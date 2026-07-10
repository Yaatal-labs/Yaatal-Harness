//! CONTROL-LOOP slice 5: the L1 proposal artifact (see `docs/CONTROL-LOOP.md` §6,
//! "ADJUSTMENT — the self-improvement segment", and the autonomy ladder).
//!
//! A [`ConfigProposal`] is **data, not a config change**. At L1 the only consumer is a
//! human: proposals land in a [`JsonlProposalStore`] file, a person reads the change,
//! the rationale, and the evidence, and decides. Nothing in this module (or anywhere
//! else in the workspace) applies a proposal automatically — L2 gated auto-apply is a
//! separate, deliberate promotion this code does not implement.
//!
//! This module lives in `yaatal-audit` rather than `yaatal-evals` because a proposal is
//! derived from and stored alongside audit events (same JSONL pattern, same `AuditEvent`
//! input type, same "append-only artifact a human queries" character), while
//! `yaatal-evals` stays pure scoring. CONTROL-LOOP.md itself places the artifact "in the
//! audit store" as its natural home.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{digest, AuditError, AuditEvent};

/// The concrete change a proposal suggests. Small and closed on purpose: the named
/// variants are the config-class knobs the L0 tenant actually has (timeouts, the
/// allowlist, the spend cap); [`ProposalChange::Other`] is the escape hatch for anything
/// a future generator wants to suggest without a schema change.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ProposalChange {
    RaiseTimeout {
        tool: String,
        from_ms: u64,
        to_ms: u64,
    },
    ReviewAllowlist {
        tool: String,
        denial_count: usize,
    },
    RaiseSpendCap {
        from: f64,
        to: f64,
    },
    Other {
        key: String,
        value: String,
    },
}

/// Lifecycle of a proposal. Machine-written proposals always start [`Proposed`]
/// (`Proposed`); the other two states exist only so a reviewing human has somewhere to
/// record their decision — no code in this workspace transitions a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProposalStatus {
    Proposed,
    /// Named to match the Engine review API's vocabulary (`Approved`, not
    /// `Accepted`) so the two systems never need a status adapter. The serde
    /// alias keeps pre-rename `proposals.jsonl` files readable.
    #[serde(alias = "Accepted")]
    Approved,
    Rejected,
}

/// The L1 artifact: a suggested config change plus the runs and human-readable evidence
/// that motivated it. See the module docs for what "L1" means (a human is the only
/// consumer).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConfigProposal {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    /// The runs whose events motivated this proposal.
    pub source_run_ids: Vec<Uuid>,
    pub change: ProposalChange,
    pub rationale: String,
    /// One human-readable line per supporting observation.
    pub evidence: Vec<String>,
    pub status: ProposalStatus,
}

impl ConfigProposal {
    /// A fresh `Proposed` proposal stamped with a new id and `created_at = now`.
    pub fn new(
        source_run_ids: Vec<Uuid>,
        change: ProposalChange,
        rationale: impl Into<String>,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            source_run_ids,
            change,
            rationale: rationale.into(),
            evidence,
            status: ProposalStatus::Proposed,
        }
    }
}

/// Append-only JSON-lines store for proposals — the same pattern as
/// [`JsonlAuditStore`](crate::JsonlAuditStore), minus the trait (there is exactly one
/// proposal backend at L1, so a trait would be an abstraction nobody asked for).
///
/// ponytail: sync std-fs I/O and read-everything queries, same L0 ceiling as the audit
/// JSONL store. Upgrade path: Postgres alongside the audit events if proposal volume
/// ever outgrows a flat file a human `jq`s through.
pub struct JsonlProposalStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl JsonlProposalStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, proposal: &ConfigProposal) -> Result<(), AuditError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(proposal)?)?;
        file.flush()?;
        Ok(())
    }

    /// All proposals in the file, oldest first. Missing file = no proposals yet.
    pub fn read_all(&self) -> Result<Vec<ConfigProposal>, AuditError> {
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let reader = BufReader::new(file);
        let mut proposals = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            proposals.push(serde_json::from_str(&line)?);
        }
        Ok(proposals)
    }
}

// =============================================================================
// RULE-BASED GENERATOR
// =============================================================================

/// One run's worth of events, as the generator consumes them. Callers assemble these
/// from an `AuditStore` (group events by `run_id`), ordered **oldest run first**.
#[derive(Debug, Clone)]
pub struct RunEvents {
    pub run_id: Uuid,
    pub events: Vec<AuditEvent>,
}

/// Thresholds for [`generate_proposals`]. Deliberately dumb — two rules, no scoring, no
/// trend fitting. This is L1 *suggest*: the rules exist to surface a pattern to a
/// human, not to be right about the fix.
#[derive(Debug, Clone)]
pub struct ProposalRules {
    /// A tool must show the pattern in at least this many runs (consecutive, for the
    /// timeout rule) before a proposal is emitted.
    pub min_runs: usize,
    /// The per-invocation timeout the runs were executed under, in milliseconds. Used
    /// to recognize timeout events (see [`generate_proposals`]) and as
    /// `RaiseTimeout::from_ms`.
    pub timeout_ms: u64,
}

/// `AuditedExec` records a timed-out invocation with this exact output string (then
/// digested). Recomputing the digest here is how the generator recognizes a timeout
/// without the audit trail ever storing raw payloads.
///
/// ponytail: digest-matching couples this rule to `AuditedExec`'s timeout message and to
/// `digest`'s cross-version stability caveat (a JSONL written by a binary built with a
/// different Rust toolchain may hash the same string differently, silently missing old
/// timeouts — never falsely detecting them). Fine for one runner writing and reading its
/// own trail; upgrade path is an explicit `timed_out` flag on `AuditEvent` if timeout
/// detection ever needs to survive toolchain upgrades.
fn timeout_digest(timeout_ms: u64) -> String {
    digest(&format!("timed out after {timeout_ms}ms"))
}

/// Rule-based L1 proposal generator over recent runs (oldest first). Two rules, both
/// deliberately dumb (see [`ProposalRules`]):
///
/// 1. **Consecutive timeouts → `RaiseTimeout`.** If the same tool has a timeout event
///    in each of the most recent `min_runs` runs, propose doubling its timeout.
/// 2. **Repeated denials → `ReviewAllowlist`.** If the same tool was policy-denied in
///    at least `min_runs` of the given runs (not necessarily consecutive — a denial is
///    a configuration mismatch, not a flake), propose a human review of the allowlist.
///
/// A condition that persists re-fires on every generation pass; the generator does not
/// dedupe against previously stored proposals. At L1 that is a feature — repeats tell
/// the reviewing human the condition still holds.
pub fn generate_proposals(runs: &[RunEvents], rules: &ProposalRules) -> Vec<ConfigProposal> {
    let mut proposals = Vec::new();
    if runs.is_empty() || rules.min_runs == 0 {
        return proposals;
    }

    let timeout_marker = timeout_digest(rules.timeout_ms);
    let mut tools: Vec<String> = runs
        .iter()
        .flat_map(|r| r.events.iter().map(|e| e.action_name.clone()))
        .collect();
    tools.sort();
    tools.dedup();

    for tool in &tools {
        // Rule 1: timeout in each of the trailing `min_runs` runs.
        if runs.len() >= rules.min_runs {
            let trailing = &runs[runs.len() - rules.min_runs..];
            let timed_out_in_all = trailing.iter().all(|run| {
                run.events.iter().any(|e| {
                    e.action_name == *tool && !e.success && e.output_digest == timeout_marker
                })
            });
            if timed_out_in_all {
                proposals.push(ConfigProposal::new(
                    trailing.iter().map(|r| r.run_id).collect(),
                    ProposalChange::RaiseTimeout {
                        tool: tool.clone(),
                        from_ms: rules.timeout_ms,
                        to_ms: rules.timeout_ms * 2,
                    },
                    format!(
                        "'{tool}' timed out in each of the last {} runs at {}ms; \
                         suggest doubling the timeout",
                        rules.min_runs, rules.timeout_ms
                    ),
                    trailing
                        .iter()
                        .map(|r| format!("run {}: '{tool}' timed out", r.run_id))
                        .collect(),
                ));
            }
        }

        // Rule 2: policy-denied in >= `min_runs` of the given runs.
        let denied_runs: Vec<&RunEvents> = runs
            .iter()
            .filter(|run| {
                run.events.iter().any(|e| {
                    e.action_name == *tool && e.policy_verdicts.iter().any(|v| v.is_deny())
                })
            })
            .collect();
        if denied_runs.len() >= rules.min_runs {
            let denial_count = runs
                .iter()
                .flat_map(|r| r.events.iter())
                .filter(|e| e.action_name == *tool && e.policy_verdicts.iter().any(|v| v.is_deny()))
                .count();
            proposals.push(ConfigProposal::new(
                denied_runs.iter().map(|r| r.run_id).collect(),
                ProposalChange::ReviewAllowlist {
                    tool: tool.clone(),
                    denial_count,
                },
                format!(
                    "'{tool}' was denied by policy in {} of the last {} runs \
                     ({denial_count} denials total); either allowlist it or stop calling it",
                    denied_runs.len(),
                    runs.len()
                ),
                denied_runs
                    .iter()
                    .map(|r| format!("run {}: '{tool}' denied by policy", r.run_id))
                    .collect(),
            ));
        }
    }

    proposals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionKind, AuditEvent, PolicyVerdict};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const TIMEOUT_MS: u64 = 5_000;

    fn rules() -> ProposalRules {
        ProposalRules {
            min_runs: 3,
            timeout_ms: TIMEOUT_MS,
        }
    }

    fn raw_event(run_id: Uuid, tool: &str, success: bool, output_digest: String) -> AuditEvent {
        AuditEvent {
            event_id: Uuid::new_v4(),
            run_id,
            actor: "harness:test".to_string(),
            action_kind: ActionKind::ToolCall,
            action_name: tool.to_string(),
            input_digest: digest("in"),
            output_digest,
            success,
            cost: None,
            latency_ms: 10,
            policy_verdicts: Vec::new(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
        }
    }

    fn ok_event(run_id: Uuid, tool: &str) -> AuditEvent {
        raw_event(run_id, tool, true, digest("out"))
    }

    fn timeout_event(run_id: Uuid, tool: &str) -> AuditEvent {
        raw_event(run_id, tool, false, timeout_digest(TIMEOUT_MS))
    }

    fn denied_event(run_id: Uuid, tool: &str) -> AuditEvent {
        let mut e = raw_event(run_id, tool, false, digest("denied"));
        e.policy_verdicts = vec![PolicyVerdict::Deny("not allowlisted".to_string())];
        e
    }

    fn run(events: Vec<AuditEvent>) -> RunEvents {
        RunEvents {
            run_id: events
                .first()
                .map(|e| e.run_id)
                .unwrap_or_else(Uuid::new_v4),
            events,
        }
    }

    fn run_with(tool_event: fn(Uuid, &str) -> AuditEvent, tool: &str) -> RunEvents {
        let run_id = Uuid::new_v4();
        run(vec![ok_event(run_id, "other"), tool_event(run_id, tool)])
    }

    #[test]
    fn consecutive_timeouts_emit_raise_timeout() {
        let runs = vec![
            run_with(timeout_event, "yaatal"),
            run_with(timeout_event, "yaatal"),
            run_with(timeout_event, "yaatal"),
        ];

        let proposals = generate_proposals(&runs, &rules());

        assert_eq!(proposals.len(), 1);
        let p = &proposals[0];
        assert_eq!(p.status, ProposalStatus::Proposed);
        assert_eq!(p.source_run_ids.len(), 3);
        assert_eq!(p.evidence.len(), 3);
        match &p.change {
            ProposalChange::RaiseTimeout {
                tool,
                from_ms,
                to_ms,
            } => {
                assert_eq!(tool, "yaatal");
                assert_eq!(*from_ms, TIMEOUT_MS);
                assert_eq!(*to_ms, TIMEOUT_MS * 2);
            }
            other => panic!("expected RaiseTimeout, got {other:?}"),
        }
    }

    #[test]
    fn a_break_in_the_timeout_streak_emits_nothing() {
        // Timed out, then recovered, then timed out: not consecutive in the trailing 3.
        let runs = vec![
            run_with(timeout_event, "yaatal"),
            run_with(ok_event, "yaatal"),
            run_with(timeout_event, "yaatal"),
        ];

        assert!(generate_proposals(&runs, &rules()).is_empty());
    }

    #[test]
    fn healthy_runs_emit_no_proposals() {
        let runs = vec![
            run_with(ok_event, "yaatal"),
            run_with(ok_event, "yaatal"),
            run_with(ok_event, "yaatal"),
        ];

        assert!(generate_proposals(&runs, &rules()).is_empty());
    }

    #[test]
    fn repeated_denials_emit_review_allowlist_even_non_consecutive() {
        let runs = vec![
            run_with(denied_event, "rm"),
            run_with(ok_event, "yaatal"),
            run_with(denied_event, "rm"),
            run_with(denied_event, "rm"),
        ];

        let proposals = generate_proposals(&runs, &rules());

        assert_eq!(proposals.len(), 1);
        match &proposals[0].change {
            ProposalChange::ReviewAllowlist { tool, denial_count } => {
                assert_eq!(tool, "rm");
                assert_eq!(*denial_count, 3);
            }
            other => panic!("expected ReviewAllowlist, got {other:?}"),
        }
        assert_eq!(proposals[0].source_run_ids.len(), 3);
    }

    #[test]
    fn fewer_runs_than_min_runs_emits_nothing() {
        let runs = vec![
            run_with(timeout_event, "yaatal"),
            run_with(timeout_event, "yaatal"),
        ];
        assert!(generate_proposals(&runs, &rules()).is_empty());
    }

    #[test]
    fn proposals_jsonl_round_trips() {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("yaatal-proposals-{nanos}-{n}.jsonl"));

        let store = JsonlProposalStore::new(&path);
        assert!(store.read_all().expect("read empty").is_empty());

        let p1 = ConfigProposal::new(
            vec![Uuid::new_v4()],
            ProposalChange::RaiseTimeout {
                tool: "yaatal".to_string(),
                from_ms: 5_000,
                to_ms: 10_000,
            },
            "test rationale",
            vec!["evidence line".to_string()],
        );
        let p2 = ConfigProposal::new(
            vec![Uuid::new_v4(), Uuid::new_v4()],
            ProposalChange::Other {
                key: "some.key".to_string(),
                value: "some-value".to_string(),
            },
            "other rationale",
            vec![],
        );
        store.append(&p1).expect("append p1");
        store.append(&p2).expect("append p2");

        // Reopen: a fresh store at the same path sees both, in order, intact.
        let reopened = JsonlProposalStore::new(&path);
        let all = reopened.read_all().expect("read_all");
        assert_eq!(all, vec![p1, p2]);

        let _ = std::fs::remove_file(&path);
    }
}
