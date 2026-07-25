//! Audit segment of the Yaatal control loop.
//!
//! This crate started as **CONTROL-LOOP slice 1** (see `docs/CONTROL-LOOP.md` in the
//! workspace root): the audit spine. It defines the `AuditEvent` schema, the `AuditStore`
//! trait (with an in-memory and a JSONL-file implementation), an adapter that turns
//! `yaatal-tools`' existing `Observer`/`PipelineEvent` hook into `AuditEvent`s, and (slice
//! 2) a [`metrics`] module that rolls those events up into per-run / per-time-range
//! aggregates.
//!
//! **Autonomy level: L0 (observe + aggregate).** This crate records what already
//! happened and computes plain aggregates over it. It contains:
//! - no policy *enforcement* (that is `yaatal-policy`'s `ToolPolicy`, CONTROL-LOOP
//!   slice 3 — `PolicyVerdict` below is the concrete type `yaatal-policy` now records on
//!   `AuditEvent::policy_verdicts`, but this crate never decides Allow/Deny itself),
//! - no eval scoring (CONTROL-LOOP slice 4, `yaatal-evals`),
//! - no config mutation. The [`proposals`] module (CONTROL-LOOP slice 5) *generates*
//!   L1 `ConfigProposal` artifacts — data a human reviews — but nothing in this
//!   workspace applies one.
//!
//! Every event this crate writes is either empty of `policy_verdicts` or carries verdicts
//! handed to it by a caller — this crate never decides Allow/Deny itself.

pub mod metrics;
#[cfg(feature = "postgres")]
mod pg;
pub mod proposals;

#[cfg(feature = "postgres")]
pub use pg::PgAuditStore;

use std::collections::hash_map::DefaultHasher;
use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use yaatal_core::{Observer, ObserverError, PipelineEvent};

// =============================================================================
// DIGEST
// =============================================================================

/// Digest a payload without retaining it anywhere. `AuditEvent` stores only digests of
/// input/output, never the raw text — see CONTROL-LOOP.md's "digests, not payloads"
/// invariant (audit records must not become a second copy of user data).
///
/// These digests are **correlation identifiers only**: good enough to tell "same input
/// again" from "different input" within one audit trail, not a cryptographic commitment.
/// They are not collision-resistant, and — because this uses `std::hash::Hasher`'s
/// default algorithm (currently SipHash-1-3) rather than a documented, version-stable
/// algorithm — not guaranteed stable across Rust toolchain versions either. Do not treat
/// the `stdhash:` prefix as a promise the same input hashes the same way after a rustc
/// upgrade.
///
/// ponytail: `sha2` is not in this workspace's dependency tree (checked `Cargo.lock`), so
/// rather than add a new dependency for an L0 scaffold this uses the standard library's
/// `Hasher`. It still satisfies "never leak the raw payload" — a length+prefix digest
/// would not. Upgrade path: swap this function's body for `sha2::Sha256` if a workspace
/// crate ever already depends on it, or once real collision-resistance / cross-version
/// stability is needed (e.g. content-addressed dedup across untrusted input, or digests
/// compared across a Rust upgrade).
pub fn digest(payload: &str) -> String {
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    format!("stdhash:{:016x}", hasher.finish())
}

/// Convert a `u64` millisecond duration to the `i64` `chrono::Duration::milliseconds`
/// wants, clamping instead of wrapping. A `u64` latency larger than `i64::MAX` cannot
/// come from a real clock reading in this process's lifetime, but a clamp is one line
/// and turns "would-never-happen" into "provably can't produce a bogus negative
/// `started_at`" rather than trusting the never-happens assumption.
fn clamp_ms_to_i64(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

// =============================================================================
// SCHEMA
// =============================================================================

/// The category of action an `AuditEvent` records, per CONTROL-LOOP.md's proposed schema.
///
/// `Hash` is derived (beyond what slice 1 needed) so [`metrics::compute`] can group
/// events into a `HashMap<ActionKind, usize>` without a second parallel key type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ActionKind {
    ModelCall,
    ToolCall,
    PolicyCheck,
    EvalScore,
}

/// The verdict a policy gate reaches about an action.
///
/// `yaatal-policy`'s `ToolPolicy` (CONTROL-LOOP slice 3 — tool allowlist + per-run spend
/// cap) is the gate that produces these; this crate only defines the type so
/// `AuditEvent::policy_verdicts` has somewhere to put it, and never decides Allow/Deny
/// itself (docs/CONTROL-LOOP.md: "empty if this event predates a gate").
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PolicyVerdict {
    Allow,
    Deny(String),
    AllowWithCap(f64),
}

impl PolicyVerdict {
    /// `true` for `Deny` — the one branch that means "the action must not execute."
    pub fn is_deny(&self) -> bool {
        matches!(self, PolicyVerdict::Deny(_))
    }
}

/// One audited action: a model call, a tool call, a policy check, or an eval score.
///
/// Field list follows CONTROL-LOOP.md's proposed schema, with two additions the doc
/// calls "illustrative, not final": `success` (mirrors the `success` flag
/// `PipelineEvent::ToolCompleted` already carries — dropping it would make failed calls
/// indistinguishable from succeeded ones), and `cost` typed as a plain `Option<f64>`
/// (a boring unit-agnostic cost estimate; the doc left the `Cost` type unspecified).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub actor: String,
    pub action_kind: ActionKind,
    pub action_name: String,
    pub input_digest: String,
    pub output_digest: String,
    pub success: bool,
    pub cost: Option<f64>,
    pub latency_ms: u64,
    pub policy_verdicts: Vec<PolicyVerdict>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

/// Builder that fills in `event_id`/timestamps/digests for you. Intended for the "wrap a
/// call" style: build it before doing the work, do the work, then `.finish(...)` — the
/// elapsed time between the two becomes `latency_ms`.
///
/// For recording a call that has *already* finished (e.g. converting a historical
/// `PipelineEvent`), construct `AuditEvent` directly instead — see `audit_tool_call` /
/// `audit_model_call` below.
pub struct AuditEventBuilder {
    run_id: Uuid,
    actor: String,
    action_kind: ActionKind,
    action_name: String,
    started_at: DateTime<Utc>,
    cost: Option<f64>,
    policy_verdicts: Vec<PolicyVerdict>,
}

impl AuditEventBuilder {
    pub fn new(
        run_id: Uuid,
        actor: impl Into<String>,
        action_kind: ActionKind,
        action_name: impl Into<String>,
    ) -> Self {
        Self {
            run_id,
            actor: actor.into(),
            action_kind,
            action_name: action_name.into(),
            started_at: Utc::now(),
            cost: None,
            policy_verdicts: Vec::new(),
        }
    }

    pub fn cost(mut self, cost: f64) -> Self {
        self.cost = Some(cost);
        self
    }

    pub fn policy_verdict(mut self, verdict: PolicyVerdict) -> Self {
        self.policy_verdicts.push(verdict);
        self
    }

    /// Digest `input`/`output` (never store them raw) and stamp `completed_at` = now.
    pub fn finish(self, input: &str, output: &str, success: bool) -> AuditEvent {
        let completed_at = Utc::now();
        let latency_ms = (completed_at - self.started_at).num_milliseconds().max(0) as u64;
        AuditEvent {
            event_id: Uuid::new_v4(),
            run_id: self.run_id,
            actor: self.actor,
            action_kind: self.action_kind,
            action_name: self.action_name,
            input_digest: digest(input),
            output_digest: digest(output),
            success,
            cost: self.cost,
            latency_ms,
            policy_verdicts: self.policy_verdicts,
            started_at: self.started_at,
            completed_at,
        }
    }
}

// =============================================================================
// STORE
// =============================================================================

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit store io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit event (de)serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[cfg(feature = "postgres")]
    #[error("audit store database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Append-only store for `AuditEvent`s: append, and query by run/time-range/count.
/// Mirrors the shape of `yaatal_core::MemoryStore` / `yaatal_memory::InMemoryStore`.
#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError>;
    async fn by_run(&self, run_id: Uuid) -> Result<Vec<AuditEvent>, AuditError>;
    async fn in_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AuditEvent>, AuditError>;
    async fn count(&self) -> Result<usize, AuditError>;
}

/// In-memory `AuditStore`, for tests. Not durable — dropped with the process.
#[derive(Default)]
pub struct MemoryAuditStore {
    events: Mutex<Vec<AuditEvent>>,
}

impl MemoryAuditStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn snapshot(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl AuditStore for MemoryAuditStore {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event);
        Ok(())
    }

    async fn by_run(&self, run_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        Ok(self
            .snapshot()
            .into_iter()
            .filter(|e| e.run_id == run_id)
            .collect())
    }

    async fn in_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        Ok(self
            .snapshot()
            .into_iter()
            .filter(|e| e.started_at >= from && e.started_at <= to)
            .collect())
    }

    async fn count(&self) -> Result<usize, AuditError> {
        Ok(self.snapshot().len())
    }
}

/// Append-only JSON-lines `AuditStore` (one `AuditEvent` per line). Durable across
/// process restarts; reads stream the file rather than holding it all in a field.
///
/// ponytail: JSONL-on-disk is the L0 ceiling. Upgrade path: `PgAuditStore` (behind the
/// `postgres` feature — reuses the Postgres the Engine already hosts) once query volume or
/// concurrent-writer needs outgrow a flat file.
pub struct JsonlAuditStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl JsonlAuditStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_all(&self) -> Result<Vec<AuditEvent>, AuditError> {
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str(&line)?);
        }
        Ok(events)
    }
}

#[async_trait]
impl AuditStore for JsonlAuditStore {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(&event)?)?;
        file.flush()?;
        Ok(())
    }

    async fn by_run(&self, run_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        Ok(self
            .read_all()?
            .into_iter()
            .filter(|e| e.run_id == run_id)
            .collect())
    }

    async fn in_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        Ok(self
            .read_all()?
            .into_iter()
            .filter(|e| e.started_at >= from && e.started_at <= to)
            .collect())
    }

    async fn count(&self) -> Result<usize, AuditError> {
        Ok(self.read_all()?.len())
    }
}

// =============================================================================
// INTEGRATION HOOK
// =============================================================================

/// A digest placeholder used when the source event carries no payload to hash — see
/// `ToolAuditObserver` below. Not a hash of anything: it is a fixed sentinel so it is
/// never confused with a real digest of empty input.
const NO_PAYLOAD: &str = "unavailable:pipeline-event-carries-no-payload";

/// Adapts `yaatal_core::Observer` / `PipelineEvent` — the hook `yaatal_tools::ToolExecutor`
/// already emits `ToolStarted`/`ToolCompleted` through — into `AuditEvent`s appended to an
/// `AuditStore`. This is CONTROL-LOOP slice 1's "new `Observer` implementation, not a new
/// call site": register it on a `ToolExecutor` and every tool call gets audited for free.
///
/// ```ignore
/// let store = Arc::new(MemoryAuditStore::new());
/// let executor = ToolExecutor::new();
/// executor.add_observer(Arc::new(ToolAuditObserver::new(store.clone(), "engine:my-cron")));
/// ```
///
/// ponytail: `PipelineEvent::ToolCompleted` carries `tool_name`/`duration_ms`/`success` but
/// not the tool's actual arguments or result content (yaatal-tools doesn't thread that
/// through the event), so `input_digest`/`output_digest` here are metadata-only, not real
/// per-call digests. Callers who have the actual strings (e.g. a call site not going
/// through `ToolExecutor`) should use `audit_tool_call` instead. Upgrade path: extend
/// `PipelineEvent::ToolCompleted` with digested arguments/result if per-call content
/// auditing becomes load-bearing.
///
/// Also: `PipelineEvent` carries a `request_id: String`, not a `run_id: Uuid`. This adapter
/// parses `request_id` as a UUID when possible and falls back to a fresh `Uuid::new_v4()`
/// otherwise — which means non-UUID request ids won't group across events. Runtimes that
/// want grouping should mint UUID-shaped request ids.
pub struct ToolAuditObserver {
    store: Arc<dyn AuditStore>,
    actor: String,
}

impl ToolAuditObserver {
    pub fn new(store: Arc<dyn AuditStore>, actor: impl Into<String>) -> Self {
        Self {
            store,
            actor: actor.into(),
        }
    }
}

#[async_trait]
impl Observer for ToolAuditObserver {
    async fn on_event(&self, event: PipelineEvent) -> Result<(), ObserverError> {
        let PipelineEvent::ToolCompleted {
            tool_name,
            request_id,
            duration_ms,
            success,
        } = event
        else {
            return Ok(());
        };

        let run_id = Uuid::parse_str(&request_id).unwrap_or_else(|_| Uuid::new_v4());
        let completed_at = Utc::now();
        let started_at = completed_at - Duration::milliseconds(clamp_ms_to_i64(duration_ms));

        let audit_event = AuditEvent {
            event_id: Uuid::new_v4(),
            run_id,
            actor: self.actor.clone(),
            action_kind: ActionKind::ToolCall,
            action_name: tool_name,
            input_digest: NO_PAYLOAD.to_string(),
            output_digest: digest(&format!("success={success}")),
            success,
            cost: None,
            latency_ms: duration_ms,
            policy_verdicts: Vec::new(),
            started_at,
            completed_at,
        };

        self.store
            .append(audit_event)
            .await
            .map_err(|e| ObserverError::Other(e.to_string()))
    }
}

/// Record a tool call directly, for callers that have the real arguments/result strings
/// (unlike `ToolAuditObserver`, which only sees `PipelineEvent`'s metadata). Also the hook
/// for call sites that don't go through `ToolExecutor` at all.
///
/// L0: this only appends the resulting `AuditEvent`. It never gates, retries, or otherwise
/// touches execution — the call has already happened by the time this runs.
pub async fn audit_tool_call(
    store: &dyn AuditStore,
    run_id: Uuid,
    actor: &str,
    tool_name: &str,
    arguments: &str,
    result: Result<&str, &str>,
    latency_ms: u64,
) -> Result<(), AuditError> {
    let (output, success) = match result {
        Ok(content) => (content, true),
        Err(err) => (err, false),
    };
    let completed_at = Utc::now();
    let started_at = completed_at - Duration::milliseconds(clamp_ms_to_i64(latency_ms));
    let event = AuditEvent {
        event_id: Uuid::new_v4(),
        run_id,
        actor: actor.to_string(),
        action_kind: ActionKind::ToolCall,
        action_name: tool_name.to_string(),
        input_digest: digest(arguments),
        output_digest: digest(output),
        success,
        cost: None,
        latency_ms,
        policy_verdicts: Vec::new(),
        started_at,
        completed_at,
    };
    store.append(event).await
}

/// Record a model call directly. `yaatal_models::LlmProvider` has no `Observer` wiring
/// today (unlike `yaatal_tools::ToolExecutor` — grep confirms nothing emits
/// `PipelineEvent::LlmCall*` yet), so there is no existing hook to adapt for model calls;
/// this free function is the documented attachment point until `LlmProvider::chat`/
/// `::embed` grow one (CONTROL-LOOP slice 2). Call it around a `chat`/`embed` call site.
#[allow(clippy::too_many_arguments)]
pub async fn audit_model_call(
    store: &dyn AuditStore,
    run_id: Uuid,
    actor: &str,
    provider_and_model: &str,
    input: &str,
    result: Result<&str, &str>,
    latency_ms: u64,
    cost: Option<f64>,
) -> Result<(), AuditError> {
    let (output, success) = match result {
        Ok(content) => (content, true),
        Err(err) => (err, false),
    };
    let completed_at = Utc::now();
    let started_at = completed_at - Duration::milliseconds(clamp_ms_to_i64(latency_ms));
    let event = AuditEvent {
        event_id: Uuid::new_v4(),
        run_id,
        actor: actor.to_string(),
        action_kind: ActionKind::ModelCall,
        action_name: provider_and_model.to_string(),
        input_digest: digest(input),
        output_digest: digest(output),
        success,
        cost,
        latency_ms,
        policy_verdicts: Vec::new(),
        started_at,
        completed_at,
    };
    store.append(event).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_jsonl_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("yaatal-audit-{nanos}-{n}.jsonl"))
    }

    #[test]
    fn event_serialization_round_trips() {
        let run_id = Uuid::new_v4();
        let event =
            AuditEventBuilder::new(run_id, "engine:test-actor", ActionKind::ToolCall, "shell")
                .cost(0.002)
                .policy_verdict(PolicyVerdict::Allow)
                .finish(r#"{"command":"echo hi"}"#, "hi\n", true);

        let json = serde_json::to_string(&event).expect("serialize");
        let round_tripped: AuditEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(event, round_tripped);
        assert_eq!(round_tripped.action_kind, ActionKind::ToolCall);
        assert_eq!(round_tripped.run_id, run_id);
        assert!(round_tripped.success);
        assert_ne!(round_tripped.input_digest, round_tripped.output_digest);
    }

    #[tokio::test]
    async fn jsonl_store_survives_reopen() {
        let path = temp_jsonl_path();
        let run_id = Uuid::new_v4();

        {
            let store = JsonlAuditStore::new(&path);
            let event =
                AuditEventBuilder::new(run_id, "engine:cron", ActionKind::ToolCall, "shell")
                    .finish("in", "out", true);
            store.append(event).await.expect("append");
        }

        // Reopen: a fresh store instance pointed at the same path should see the event.
        let reopened = JsonlAuditStore::new(&path);
        let events = reopened.by_run(run_id).await.expect("by_run");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action_name, "shell");
        assert_eq!(reopened.count().await.expect("count"), 1);

        // A second append should land alongside the first, not clobber it.
        let second = AuditEventBuilder::new(run_id, "engine:cron", ActionKind::ToolCall, "git")
            .finish("in2", "out2", false);
        reopened.append(second).await.expect("append 2");

        let after = JsonlAuditStore::new(&path);
        assert_eq!(after.count().await.expect("count"), 2);
        let by_run = after.by_run(run_id).await.expect("by_run");
        assert_eq!(by_run.len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn memory_store_queries_by_run_id() {
        let store = MemoryAuditStore::new();
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        store
            .append(
                AuditEventBuilder::new(run_a, "engine:cron", ActionKind::ToolCall, "shell")
                    .finish("a-in", "a-out", true),
            )
            .await
            .expect("append a1");
        store
            .append(
                AuditEventBuilder::new(run_a, "engine:cron", ActionKind::ModelCall, "openai/gpt")
                    .finish("a-in-2", "a-out-2", true),
            )
            .await
            .expect("append a2");
        store
            .append(
                AuditEventBuilder::new(run_b, "studio:live", ActionKind::ToolCall, "shell")
                    .finish("b-in", "b-out", false),
            )
            .await
            .expect("append b1");

        assert_eq!(store.count().await.expect("count"), 3);

        let a_events = store.by_run(run_a).await.expect("by_run a");
        assert_eq!(a_events.len(), 2);
        assert!(a_events.iter().all(|e| e.run_id == run_a));

        let b_events = store.by_run(run_b).await.expect("by_run b");
        assert_eq!(b_events.len(), 1);
        assert_eq!(b_events[0].actor, "studio:live");
        assert!(!b_events[0].success);
    }

    #[tokio::test]
    async fn tool_executor_observer_writes_audit_event() {
        use yaatal_core::RequestContext;
        use yaatal_tools::{BuiltinTool, ToolExecutor};

        let store: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::new());
        let executor = ToolExecutor::new();
        executor
            .add_observer(Arc::new(ToolAuditObserver::new(
                store.clone(),
                "engine:test-actor",
            )))
            .await;
        executor
            .register_builtin(BuiltinTool::Shell)
            .await
            .expect("local-shell feature enabled for this dev-dependency");

        let ctx = RequestContext::new(Uuid::new_v4().to_string());
        let result = executor
            .execute(&ctx, "shell", r#"{"command":"echo hi"}"#)
            .await
            .expect("tool executes");
        assert!(result.success);

        let run_id = Uuid::parse_str(&ctx.request_id).expect("uuid request id");
        let events = store.by_run(run_id).await.expect("by_run");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action_kind, ActionKind::ToolCall);
        assert_eq!(events[0].action_name, "shell");
        assert!(events[0].success);
    }
}
