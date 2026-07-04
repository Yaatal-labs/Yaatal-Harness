//! CONTROL-LOOP slice 3: the tool policy gate (see `docs/CONTROL-LOOP.md` §5, "POLICY
//! GATE — checked in-path, not after the fact").
//!
//! `PolicyEngine` (this crate's existing trait, in `lib.rs`) is defined over
//! `ScoredCandidate` — ranked retrieval results. That shape doesn't fit "should this tool
//! call be allowed to run," so this module adds a narrower [`ToolPolicy`] trait, exactly
//! as CONTROL-LOOP.md sketches, plus one concrete implementation ([`ToolPolicyGate`])
//! carrying the two v1 rules the doc specifies: a tool allowlist and a per-run spend cap.
//!
//! One deviation from the doc's illustrative signature —
//! `fn check(&self, ctx, tool_name, arguments) -> PolicyVerdict` — worth calling out: the
//! spend-cap rule needs a stable `run_id` to sum `AuditEvent::cost` against, and
//! `RequestContext` only carries a `request_id: String`. `yaatal-audit`'s own adapter
//! (`ToolAuditObserver`) already documents the footgun of overloading that string as a
//! UUID (falls back to a fresh random UUID on parse failure, silently breaking
//! grouping). Rather than repeat that ambiguity here, `check` takes `run_id: Uuid`
//! explicitly. Callers that already mint UUID-shaped `request_id`s can pass
//! `Uuid::parse_str(&ctx.request_id)` straight through.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use yaatal_audit::{AuditStore, PolicyVerdict};
use yaatal_core::RequestContext;

/// Resolve the "actor" a `RequestContext` is acting as, for per-actor allowlist
/// overrides. `RequestContext` has no dedicated `actor` field (that's a concept this
/// control loop's audit/policy layers introduced, not a core type field), so this checks
/// `ctx.metadata["actor"]` first (the same free-form-metadata pattern
/// `yaatal_tools::SessionNoteTool` already uses for `session_id`), then falls back to
/// `ctx.tenant`, then to a fixed default.
fn resolve_actor(ctx: &RequestContext) -> &str {
    if let Some(actor) = ctx.metadata.get("actor") {
        return actor.as_str();
    }
    if let Some(tenant) = ctx.tenant.as_deref() {
        return tenant;
    }
    "default"
}

/// A policy gate checked *before* a tool call executes (CONTROL-LOOP.md §5). Unlike
/// `PolicyEngine` (ranked-item filtering, evaluated over already-produced results), this
/// runs ahead of the action and its answer decides whether the action happens at all.
#[async_trait]
pub trait ToolPolicy: Send + Sync {
    /// `arguments` is whatever the caller wants to gate on beyond the tool name (e.g. a
    /// JSON-args string, or a `"program arg1 arg2"` command line) — implementations in
    /// this module ignore it; it exists for future rules (e.g. argument-shape checks)
    /// that aren't in the v1 scope CONTROL-LOOP.md specifies.
    async fn check(
        &self,
        ctx: &RequestContext,
        run_id: Uuid,
        tool_name: &str,
        arguments: &str,
    ) -> PolicyVerdict;
}

/// The two CONTROL-LOOP.md v1 rules, combined in one gate: a tool allowlist (checked
/// first — a call for a tool nobody may run should never even reach the spend-cap query),
/// then an optional per-run spend cap.
pub struct ToolPolicyGate {
    default_allowed: HashSet<String>,
    per_actor_allowed: HashMap<String, HashSet<String>>,
    spend_cap: Option<f64>,
    store: Arc<dyn AuditStore>,
}

impl ToolPolicyGate {
    /// `default_allowed` is the allowlist used for any actor without a per-actor
    /// override. `store` is queried for the spend-cap rule; pass `spend_cap: None` to
    /// disable that rule entirely (allowlist-only gate).
    pub fn new(
        default_allowed: impl IntoIterator<Item = String>,
        spend_cap: Option<f64>,
        store: Arc<dyn AuditStore>,
    ) -> Self {
        Self {
            default_allowed: default_allowed.into_iter().collect(),
            per_actor_allowed: HashMap::new(),
            spend_cap,
            store,
        }
    }

    /// Override the allowlist for one actor (see [`resolve_actor`] for how an actor is
    /// derived from a `RequestContext`).
    pub fn with_actor_allowed(
        mut self,
        actor: impl Into<String>,
        allowed: impl IntoIterator<Item = String>,
    ) -> Self {
        self.per_actor_allowed
            .insert(actor.into(), allowed.into_iter().collect());
        self
    }

    fn allowlist_for(&self, actor: &str) -> &HashSet<String> {
        self.per_actor_allowed
            .get(actor)
            .unwrap_or(&self.default_allowed)
    }
}

#[async_trait]
impl ToolPolicy for ToolPolicyGate {
    async fn check(
        &self,
        ctx: &RequestContext,
        run_id: Uuid,
        tool_name: &str,
        _arguments: &str,
    ) -> PolicyVerdict {
        let actor = resolve_actor(ctx);
        if !self.allowlist_for(actor).contains(tool_name) {
            return PolicyVerdict::Deny(format!(
                "tool '{tool_name}' is not on the allowlist for actor '{actor}'"
            ));
        }

        let Some(cap) = self.spend_cap else {
            return PolicyVerdict::Allow;
        };

        let spent: f64 = match self.store.by_run(run_id).await {
            Ok(events) => events.iter().filter_map(|e| e.cost).sum(),
            Err(e) => return PolicyVerdict::Deny(format!("spend cap check failed: {e}")),
        };

        if spent >= cap {
            PolicyVerdict::Deny(format!(
                "run {run_id} spend cap {cap:.4} reached (already spent {spent:.4})"
            ))
        } else {
            PolicyVerdict::AllowWithCap(cap - spent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaatal_audit::{ActionKind, AuditEventBuilder, MemoryAuditStore};

    fn ctx() -> RequestContext {
        RequestContext::new(Uuid::new_v4().to_string())
    }

    #[tokio::test]
    async fn allows_a_listed_tool_with_no_spend_cap() {
        let store: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::new());
        let gate = ToolPolicyGate::new(["echo".to_string()], None, store);

        let verdict = gate.check(&ctx(), Uuid::new_v4(), "echo", "").await;
        assert!(matches!(verdict, PolicyVerdict::Allow));
    }

    #[tokio::test]
    async fn denies_a_tool_not_on_the_allowlist() {
        let store: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::new());
        let gate = ToolPolicyGate::new(["echo".to_string()], None, store);

        let verdict = gate.check(&ctx(), Uuid::new_v4(), "rm", "").await;
        assert!(verdict.is_deny());
    }

    #[tokio::test]
    async fn per_actor_allowlist_overrides_the_default() {
        let store: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::new());
        let gate = ToolPolicyGate::new(["echo".to_string()], None, store)
            .with_actor_allowed("engine:cron", ["echo".to_string(), "git".to_string()]);

        let mut privileged = ctx();
        privileged
            .metadata
            .insert("actor".to_string(), "engine:cron".to_string());

        assert!(matches!(
            gate.check(&privileged, Uuid::new_v4(), "git", "").await,
            PolicyVerdict::Allow
        ));
        // The default actor still only gets the base allowlist.
        assert!(gate
            .check(&ctx(), Uuid::new_v4(), "git", "")
            .await
            .is_deny());
    }

    #[tokio::test]
    async fn denies_once_the_run_spend_cap_is_reached() {
        let store = Arc::new(MemoryAuditStore::new());
        let run_id = Uuid::new_v4();
        store
            .append(
                AuditEventBuilder::new(run_id, "engine:cron", ActionKind::ToolCall, "echo")
                    .cost(0.6)
                    .finish("in", "out", true),
            )
            .await
            .expect("seed spend");
        store
            .append(
                AuditEventBuilder::new(run_id, "engine:cron", ActionKind::ToolCall, "echo")
                    .cost(0.5)
                    .finish("in", "out", true),
            )
            .await
            .expect("seed spend 2");

        let gate = ToolPolicyGate::new(["echo".to_string()], Some(1.0), store);

        let verdict = gate.check(&ctx(), run_id, "echo", "").await;
        assert!(verdict.is_deny());
    }

    #[tokio::test]
    async fn allow_with_cap_reports_remaining_budget() {
        let store = Arc::new(MemoryAuditStore::new());
        let run_id = Uuid::new_v4();
        store
            .append(
                AuditEventBuilder::new(run_id, "engine:cron", ActionKind::ToolCall, "echo")
                    .cost(0.4)
                    .finish("in", "out", true),
            )
            .await
            .expect("seed spend");

        let gate = ToolPolicyGate::new(["echo".to_string()], Some(1.0), store);

        let verdict = gate.check(&ctx(), run_id, "echo", "").await;
        match verdict {
            PolicyVerdict::AllowWithCap(remaining) => {
                assert!((remaining - 0.6).abs() < 1e-9);
            }
            other => panic!("expected AllowWithCap, got {other:?}"),
        }
    }
}
