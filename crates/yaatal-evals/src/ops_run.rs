//! CONTROL-LOOP slice 4: one real eval that scores an *agent run*, not a ranked list
//! (see `docs/CONTROL-LOOP.md` §4, "EVAL — one real eval scores the run").
//!
//! [`OpsRunEval`] scores one run's `AuditEvent`s — pulled from any `AuditStore` by
//! `run_id` — against four mechanically-checkable criteria. Per the doc, v1 is the
//! mechanically-checkable eval; anything requiring a judgment call (LLM-as-judge) is a
//! later milestone, not this module.
//!
//! Shape note: this crate's existing evals (`mean_reciprocal_rank`, `ndcg_at_k`) are
//! plain pure functions over data the caller already has. `OpsRunEval` keeps that
//! shape — [`OpsRunEval::evaluate`] is a pure function over a `&[AuditEvent]` slice, no
//! I/O — and only adds a struct around it because the checks carry configuration
//! (spend cap, latency threshold) that a bare function signature would turn into a
//! four-argument soup.

use uuid::Uuid;
use yaatal_audit::{metrics, AuditEvent};

/// Names of the four checks, used as `Finding::check` values. Public so callers
/// (tests, dashboards, the ops runner) can match on them without string literals
/// drifting.
pub const CHECK_RUN_COMPLETED: &str = "run_completed";
pub const CHECK_NO_POLICY_DENIALS: &str = "no_policy_denials";
pub const CHECK_SPEND_WITHIN_CAP: &str = "spend_within_cap";
pub const CHECK_P95_LATENCY: &str = "p95_latency";

/// One named check's outcome inside an [`OpsRunReport`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

/// The serializable result of evaluating one run. `score` is the fraction of checks
/// that passed (0.0–1.0); `passed` is `true` only when every check passed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpsRunReport {
    pub run_id: Uuid,
    pub score: f64,
    pub passed: bool,
    pub findings: Vec<Finding>,
}

/// Scores one operational run's audit trail. Construct with the thresholds the run was
/// supposed to stay within, then call [`evaluate`](Self::evaluate) with the run's
/// events (e.g. from `AuditStore::by_run`).
///
/// The four checks, each a named [`Finding`]:
/// 1. **run_completed** — the run produced at least one event and every event
///    succeeded (a step that failed, timed out, or was denied fails this).
/// 2. **no_policy_denials** — no event carries a `Deny` policy verdict.
/// 3. **spend_within_cap** — the run's summed `cost` is at or under `spend_cap`
///    (passes trivially when no cap is configured).
/// 4. **p95_latency** — the run's p95 event latency is at or under `p95_latency_ms`.
#[derive(Debug, Clone, PartialEq)]
pub struct OpsRunEval {
    /// Maximum total cost the run may accrue. `None` disables the spend check
    /// (it passes with a "no cap configured" detail).
    pub spend_cap: Option<f64>,
    /// Maximum acceptable p95 latency across the run's events, in milliseconds.
    pub p95_latency_ms: u64,
}

impl OpsRunEval {
    /// Evaluate `events` (one run's audit trail) into an [`OpsRunReport`]. Pure — no
    /// store access; callers fetch the slice (`AuditStore::by_run`) themselves so the
    /// same events can also feed `metrics::compute` without a second read.
    pub fn evaluate(&self, run_id: Uuid, events: &[AuditEvent]) -> OpsRunReport {
        let m = metrics::compute(events);

        let failures = events.iter().filter(|e| !e.success).count();
        let run_completed = Finding {
            check: CHECK_RUN_COMPLETED.to_string(),
            passed: m.event_count >= 1 && failures == 0,
            detail: if m.event_count == 0 {
                "run produced no audit events".to_string()
            } else {
                format!("{} events, {failures} failed", m.event_count)
            },
        };

        let denials = events
            .iter()
            .filter(|e| e.policy_verdicts.iter().any(|v| v.is_deny()))
            .count();
        let no_denials = Finding {
            check: CHECK_NO_POLICY_DENIALS.to_string(),
            passed: denials == 0,
            detail: format!("{denials} events carried a policy denial"),
        };

        let spend = match self.spend_cap {
            None => Finding {
                check: CHECK_SPEND_WITHIN_CAP.to_string(),
                passed: true,
                detail: format!("no spend cap configured (spent {:.4})", m.total_cost),
            },
            Some(cap) => Finding {
                check: CHECK_SPEND_WITHIN_CAP.to_string(),
                passed: m.total_cost <= cap,
                detail: format!("spent {:.4} of cap {cap:.4}", m.total_cost),
            },
        };

        let latency = Finding {
            check: CHECK_P95_LATENCY.to_string(),
            passed: m.p95_latency_ms <= self.p95_latency_ms,
            detail: format!(
                "p95 {}ms, threshold {}ms",
                m.p95_latency_ms, self.p95_latency_ms
            ),
        };

        let findings = vec![run_completed, no_denials, spend, latency];
        let passed_count = findings.iter().filter(|f| f.passed).count();
        OpsRunReport {
            run_id,
            score: passed_count as f64 / findings.len() as f64,
            passed: passed_count == findings.len(),
            findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaatal_audit::{ActionKind, AuditEventBuilder, PolicyVerdict};

    fn event(run_id: Uuid, cost: f64, latency_ms: u64, success: bool) -> AuditEvent {
        let mut e = AuditEventBuilder::new(run_id, "harness:test", ActionKind::ToolCall, "yaatal")
            .cost(cost)
            .finish("in", "out", success);
        e.latency_ms = latency_ms;
        e
    }

    fn eval() -> OpsRunEval {
        OpsRunEval {
            spend_cap: Some(1.0),
            p95_latency_ms: 5_000,
        }
    }

    #[test]
    fn passing_run_scores_full_marks() {
        let run_id = Uuid::new_v4();
        let events = vec![event(run_id, 0.1, 100, true), event(run_id, 0.2, 200, true)];

        let report = eval().evaluate(run_id, &events);

        assert!(report.passed);
        assert_eq!(report.score, 1.0);
        assert_eq!(report.findings.len(), 4);
        assert!(report.findings.iter().all(|f| f.passed));
        assert_eq!(report.run_id, run_id);
    }

    #[test]
    fn denial_fails_the_denials_check_and_the_run() {
        let run_id = Uuid::new_v4();
        let denied = {
            let mut e = AuditEventBuilder::new(run_id, "harness:test", ActionKind::ToolCall, "rm")
                .policy_verdict(PolicyVerdict::Deny("not allowlisted".to_string()))
                .finish("in", "denied", false);
            e.latency_ms = 5;
            e
        };
        let events = vec![event(run_id, 0.1, 100, true), denied];

        let report = eval().evaluate(run_id, &events);

        assert!(!report.passed);
        // Both run_completed (failed event) and no_policy_denials fail: 2 of 4 pass.
        assert_eq!(report.score, 0.5);
        let denial_finding = report
            .findings
            .iter()
            .find(|f| f.check == CHECK_NO_POLICY_DENIALS)
            .expect("denials finding present");
        assert!(!denial_finding.passed);
    }

    #[test]
    fn spend_over_cap_fails_only_the_spend_check() {
        let run_id = Uuid::new_v4();
        let events = vec![event(run_id, 0.7, 100, true), event(run_id, 0.7, 200, true)];

        let report = eval().evaluate(run_id, &events);

        assert!(!report.passed);
        assert_eq!(report.score, 0.75);
        let spend_finding = report
            .findings
            .iter()
            .find(|f| f.check == CHECK_SPEND_WITHIN_CAP)
            .expect("spend finding present");
        assert!(!spend_finding.passed);
        assert!(spend_finding.detail.contains("1.4000"));
    }

    #[test]
    fn empty_run_fails_run_completed_and_no_cap_passes_spend() {
        let run_id = Uuid::new_v4();
        let no_cap = OpsRunEval {
            spend_cap: None,
            p95_latency_ms: 5_000,
        };

        let report = no_cap.evaluate(run_id, &[]);

        assert!(!report.passed);
        let completed = report
            .findings
            .iter()
            .find(|f| f.check == CHECK_RUN_COMPLETED)
            .expect("run_completed finding");
        assert!(!completed.passed);
        let spend = report
            .findings
            .iter()
            .find(|f| f.check == CHECK_SPEND_WITHIN_CAP)
            .expect("spend finding");
        assert!(spend.passed, "no cap configured means the check passes");
    }

    #[test]
    fn slow_p95_fails_the_latency_check() {
        let run_id = Uuid::new_v4();
        let events = vec![event(run_id, 0.1, 60_000, true)];

        let report = eval().evaluate(run_id, &events);

        assert!(!report.passed);
        let latency = report
            .findings
            .iter()
            .find(|f| f.check == CHECK_P95_LATENCY)
            .expect("latency finding");
        assert!(!latency.passed);
    }

    #[test]
    fn report_serialization_round_trips() {
        let run_id = Uuid::new_v4();
        let report = eval().evaluate(run_id, &[event(run_id, 0.1, 100, true)]);
        let json = serde_json::to_string(&report).expect("serialize");
        let back: OpsRunReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }
}
