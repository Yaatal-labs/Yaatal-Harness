//! The Harness's first L0-operating tenant: a deterministic runbook executor that
//! drives external CLIs (the SDK's `yaatal` CLI first) through the full custody stack —
//! `ToolPolicyGate` before every spawn, `JsonlAuditStore` per invocation, `AuditMetrics`
//! + `OpsRunEval` after the run, and the L1 proposal generator over recent runs.
//!
//! **Deliberately no model call.** The first tenant needs hands, not a brain: a fixed
//! JSON runbook is executed step by step, so every moving part of the custody layer
//! (CONTROL-LOOP slices 1-5) is exercised in production without any nondeterminism in
//! what gets executed. Upgrade path: a model-driven runner that plans its own steps via
//! `yaatal-models::LlmProvider` — same `AuditedExec` custody, the runbook replaced by a
//! goal — once the deterministic loop has run long enough to be trusted.
//!
//! **The runbook IS the authorization.** The policy allowlist is derived from the
//! runbook's distinct `program`s: whoever writes/edits the runbook file decides what may
//! execute, the gate enforces that nothing else does (a mis-typed or injected program
//! name is denied and audited), and the file itself is the reviewable artifact. There is
//! no second allowlist to keep in sync with the steps.
//!
//! **A failed step records and continues.** The runner never aborts a run midway: the
//! eval is the judge of the run, and it can only judge what happened. Aborting on the
//! first failure would leave later steps unmeasured (was step 3 also broken, or just
//! never tried?), turn one flaky step into zero signal about the rest, and make the
//! nightly audit trail shape depend on which step failed. Ops consequences belong to the
//! eval verdict (exit code), not to step short-circuiting.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;
use yaatal_audit::proposals::{generate_proposals, JsonlProposalStore, ProposalRules, RunEvents};
use yaatal_audit::{metrics, AuditError, AuditStore, JsonlAuditStore};
use yaatal_core::RequestContext;
use yaatal_evals::ops_run::{OpsRunEval, OpsRunReport};
use yaatal_policy::tool_policy::ToolPolicyGate;
use yaatal_tools::audited_exec::{AuditedExec, ExecError};

/// How many runs must show a pattern before the proposal generator fires (see
/// `yaatal_audit::proposals::ProposalRules::min_runs`). A const, not a runbook knob:
/// one fewer thing to mis-configure at L1, and 3 is the smallest count where
/// "consecutive" means a streak rather than a coincidence.
const PROPOSAL_MIN_RUNS: usize = 3;

fn default_proposal_window_runs() -> usize {
    5
}

/// One step of a runbook: a specific program plus a fixed argument vector — never a
/// shell string, matching `AuditedExec`'s contract.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// The runbook file (argv\[1\] of `yaatal-ops-runner`). `deny_unknown_fields` because a
/// typo'd field name in an ops file should be a loud parse error, not a silently-ignored
/// setting.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runbook {
    pub run_name: String,
    /// Per-step timeout; a step past this is killed and audited as a failure.
    pub timeout_secs: u64,
    /// Per-run spend cap enforced by the policy gate (and checked by the eval).
    /// Omit/`null` for no cap. Note: `AuditedExec` records no per-invocation cost
    /// today, so with CLI-only steps spend stays 0 — the cap is wired end-to-end for
    /// when cost-bearing steps (model calls) arrive.
    #[serde(default)]
    pub spend_cap: Option<f64>,
    /// Eval threshold: the run fails if its p95 event latency exceeds this.
    pub p95_latency_ms: u64,
    /// Directory holding `audit.jsonl` (events) and `proposals.jsonl` (L1 artifacts).
    /// Created if missing.
    pub audit_dir: PathBuf,
    /// How many recent runs (including this one) the proposal generator looks at.
    #[serde(default = "default_proposal_window_runs")]
    pub proposal_window_runs: usize,
    pub steps: Vec<Step>,
}

impl Runbook {
    /// Parse and validate a runbook from JSON text. Validation is the trust boundary:
    /// the file is operator-authored, so a malformed one must fail here with a clear
    /// message, never half-execute.
    pub fn from_json(json: &str) -> Result<Self, RunnerError> {
        let runbook: Runbook = serde_json::from_str(json)?;
        if runbook.steps.is_empty() {
            return Err(RunnerError::Invalid("runbook has no steps".to_string()));
        }
        if runbook.timeout_secs == 0 {
            return Err(RunnerError::Invalid(
                "timeout_secs must be at least 1".to_string(),
            ));
        }
        if runbook.proposal_window_runs == 0 {
            return Err(RunnerError::Invalid(
                "proposal_window_runs must be at least 1".to_string(),
            ));
        }
        for step in &runbook.steps {
            if step.name.trim().is_empty() || step.program.trim().is_empty() {
                return Err(RunnerError::Invalid(format!(
                    "step '{}' must have a non-empty name and program",
                    step.name
                )));
            }
        }
        Ok(runbook)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("invalid runbook: {0}")]
    Invalid(String),
    #[error("runbook parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit store error: {0}")]
    Audit(#[from] AuditError),
}

/// One step's outcome in the summary. `exit_code` is `null` when the process never
/// produced one (denied by policy, killed on timeout, or failed to spawn).
#[derive(Debug, Clone, serde::Serialize)]
pub struct StepResult {
    pub name: String,
    pub exit_code: Option<i32>,
    pub ok: bool,
}

/// The single JSON value `yaatal-ops-runner` prints to stdout — same agent-first
/// contract as the SDK's `yaatal` CLI (one JSON value, exit 0/1/2, no prompts).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunSummary {
    pub run_id: Uuid,
    pub run_name: String,
    pub steps: Vec<StepResult>,
    pub metrics: metrics::AuditMetrics,
    pub eval: OpsRunReport,
    pub proposals_emitted: usize,
}

/// Execute a runbook end to end: steps through `AuditedExec` (policy + audit), then
/// metrics, eval, and the proposal pass. Child processes inherit this process's env, so
/// `YAATAL_ENGINE_URL` / `YAATAL_TOKEN` flow through to the `yaatal` CLI untouched.
pub async fn execute(runbook: &Runbook) -> Result<RunSummary, RunnerError> {
    std::fs::create_dir_all(&runbook.audit_dir)?;
    let store: Arc<JsonlAuditStore> =
        Arc::new(JsonlAuditStore::new(runbook.audit_dir.join("audit.jsonl")));
    let proposal_store = JsonlProposalStore::new(runbook.audit_dir.join("proposals.jsonl"));

    // The runbook IS the authorization: the allowlist is exactly the set of programs
    // the runbook names (BTreeSet: deduped, deterministic order).
    let allowlist: BTreeSet<String> = runbook.steps.iter().map(|s| s.program.clone()).collect();
    let gate = Arc::new(ToolPolicyGate::new(
        allowlist,
        runbook.spend_cap,
        Arc::clone(&store) as Arc<dyn AuditStore>,
    ));

    let run_id = Uuid::new_v4();
    let actor = format!("harness:ops-runner:{}", runbook.run_name);
    let exec = AuditedExec::new(
        gate,
        Arc::clone(&store) as Arc<dyn AuditStore>,
        &actor,
        Duration::from_secs(runbook.timeout_secs),
    );
    let ctx = RequestContext::new(run_id.to_string());

    let mut steps = Vec::with_capacity(runbook.steps.len());
    for step in &runbook.steps {
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        let result = exec.run(&ctx, run_id, &step.program, &args).await;
        // Failed steps record and continue — see the module docs for why the runner
        // never aborts a run midway.
        let (exit_code, ok) = match result {
            Ok(output) => (output.exit_code, output.success()),
            Err(ExecError::DeniedByPolicy(_)) | Err(ExecError::Io { .. }) => (None, false),
            // An audit-append failure is not a step failure — it means the custody
            // trail itself can't be written, so nothing downstream (metrics, eval,
            // proposals) can be trusted. Stop the run.
            Err(ExecError::Audit(e)) => return Err(e.into()),
        };
        steps.push(StepResult {
            name: step.name.clone(),
            exit_code,
            ok,
        });
    }

    // One store read feeds both the metrics rollup and the eval.
    let run_events = store.by_run(run_id).await?;
    let run_metrics = metrics::compute(&run_events);
    let eval = OpsRunEval {
        spend_cap: runbook.spend_cap,
        p95_latency_ms: runbook.p95_latency_ms,
    };
    let report = eval.evaluate(run_id, &run_events);

    let proposals = propose_over_recent_runs(&*store, runbook).await?;
    for proposal in &proposals {
        proposal_store.append(proposal)?;
    }

    Ok(RunSummary {
        run_id,
        run_name: runbook.run_name.clone(),
        steps,
        metrics: run_metrics,
        eval: report,
        proposals_emitted: proposals.len(),
    })
}

/// Group the whole audit trail into runs (ordered oldest first by each run's earliest
/// `started_at`), keep the trailing `proposal_window_runs`, and run the rule-based
/// generator over them.
///
/// ponytail: "whole audit trail" is a full-file read every run — the same L0 ceiling
/// `JsonlAuditStore` already documents. Fine at one run per day; upgrade path is a
/// store-side "last K runs" query when the trail outgrows a flat file.
async fn propose_over_recent_runs(
    store: &dyn AuditStore,
    runbook: &Runbook,
) -> Result<Vec<yaatal_audit::proposals::ConfigProposal>, RunnerError> {
    let all = store
        .in_range(chrono::DateTime::<chrono::Utc>::MIN_UTC, chrono::Utc::now())
        .await?;

    let mut runs: Vec<RunEvents> = Vec::new();
    for event in all {
        match runs.iter_mut().find(|r| r.run_id == event.run_id) {
            Some(run) => run.events.push(event),
            None => runs.push(RunEvents {
                run_id: event.run_id,
                events: vec![event],
            }),
        }
    }
    runs.sort_by_key(|r| r.events.iter().map(|e| e.started_at).min());
    if runs.len() > runbook.proposal_window_runs {
        runs.drain(..runs.len() - runbook.proposal_window_runs);
    }

    let rules = ProposalRules {
        min_runs: PROPOSAL_MIN_RUNS,
        timeout_ms: runbook.timeout_secs * 1000,
    };
    Ok(generate_proposals(&runs, &rules))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runbook_parses_and_validates() {
        let json = r#"{
            "run_name": "daily-ops",
            "timeout_secs": 60,
            "spend_cap": 1.0,
            "p95_latency_ms": 30000,
            "audit_dir": "/tmp/audit",
            "steps": [{"name": "list", "program": "yaatal", "args": ["products", "list"]}]
        }"#;
        let runbook = Runbook::from_json(json).expect("parses");
        assert_eq!(runbook.run_name, "daily-ops");
        assert_eq!(runbook.proposal_window_runs, 5, "default window");
        assert_eq!(runbook.steps.len(), 1);
    }

    #[test]
    fn empty_steps_and_unknown_fields_are_rejected() {
        let empty = r#"{
            "run_name": "x", "timeout_secs": 60, "p95_latency_ms": 1000,
            "audit_dir": "/tmp/a", "steps": []
        }"#;
        assert!(matches!(
            Runbook::from_json(empty),
            Err(RunnerError::Invalid(_))
        ));

        let typo = r#"{
            "run_name": "x", "timeout_secs": 60, "p95_latency_ms": 1000,
            "audit_dir": "/tmp/a", "step": []
        }"#;
        assert!(matches!(
            Runbook::from_json(typo),
            Err(RunnerError::Parse(_))
        ));
    }

    /// A fresh, unique audit dir under the system temp dir — no tempdir dependency
    /// (ponytail: matches the JSONL-store tests' own convention).
    fn temp_audit_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yaatal-runner-{nanos}-{}", Uuid::new_v4()))
    }

    fn runbook_with(dir: &std::path::Path, steps: Vec<Step>) -> Runbook {
        Runbook {
            run_name: "test-run".to_string(),
            timeout_secs: 30,
            spend_cap: None,
            p95_latency_ms: 60_000,
            audit_dir: dir.to_path_buf(),
            proposal_window_runs: 5,
            steps,
        }
    }

    #[tokio::test]
    async fn successful_run_audits_each_step_and_passes_eval() {
        let dir = temp_audit_dir();
        let runbook = runbook_with(
            &dir,
            vec![Step {
                name: "echo-hello".to_string(),
                program: "echo".to_string(),
                args: vec!["hello".to_string()],
            }],
        );

        let summary = execute(&runbook).await.expect("run completes");

        assert_eq!(summary.steps.len(), 1);
        assert!(summary.steps[0].ok, "echo should succeed");
        assert_eq!(summary.steps[0].exit_code, Some(0));
        assert!(summary.eval.passed, "a clean run passes the eval");
        assert!(summary.metrics.event_count >= 1, "the step was audited");

        // The custody trail is on disk, one event per invocation.
        let audit = std::fs::read_to_string(dir.join("audit.jsonl")).expect("audit written");
        assert_eq!(audit.lines().count(), 1);

        // The stdout summary must be valid single-value JSON (the agent-first contract).
        serde_json::to_string(&summary).expect("summary serializes");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn failed_step_records_continues_and_fails_the_eval() {
        let dir = temp_audit_dir();
        // A program that cannot spawn is a step failure, not a run abort: the second
        // step must still run and be audited, and the eval must fail the run.
        let missing = format!("yaatal-nonexistent-{}", Uuid::new_v4());
        let runbook = runbook_with(
            &dir,
            vec![
                Step {
                    name: "missing-first".to_string(),
                    program: missing,
                    args: vec![],
                },
                Step {
                    name: "echo-after".to_string(),
                    program: "echo".to_string(),
                    args: vec!["still-ran".to_string()],
                },
            ],
        );

        let summary = execute(&runbook)
            .await
            .expect("run completes despite a bad step");

        assert_eq!(
            summary.steps.len(),
            2,
            "the run did not abort on the failure"
        );
        assert!(!summary.steps[0].ok, "the missing program failed");
        assert!(summary.steps[1].ok, "the later step still ran");
        assert!(
            !summary.eval.passed,
            "a run with a failed step does not pass the eval"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
