//! Push `ConfigProposal`s to the Engine's review API — the missing link that closes
//! the L1 loop: proposals already land in `proposals.jsonl` for a human to `jq` through
//! (see `docs/OPS-RUNNER.md` §6); this module additionally syncs them to the Engine so
//! they show up in the control-plane dashboard (`docs/SOCIAL-GATEWAYS.md` references the
//! same `/api/harness/proposals` surface).
//!
//! **Offline-first:** with `YAATAL_ENGINE_URL` / `YAATAL_ENGINE_TOKEN` unset (the
//! default), this is a silent no-op — the runner must keep working without the Engine
//! reachable. **Re-push is the sync model, not a bug:** the Engine upserts on `id` and
//! never overwrites a decided proposal, so pushing the whole store every run is safe and
//! is how a proposal a human decided on later gets its status corrected here too (it
//! doesn't — this module has no read path back — but the push itself is always safe to
//! repeat). **A push failure never fails the run:** this fn returns nothing to propagate;
//! per-proposal errors are logged and the loop continues.

use yaatal_audit::proposals::{ConfigProposal, JsonlProposalStore, ProposalChange};

/// The JSON body shape `POST /api/harness/proposals` accepts.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct ProposalPushPayload {
    id: String,
    kind: String,
    tool: Option<String>,
    change: Option<serde_json::Value>,
    rationale: String,
    evidence_runs: usize,
}

/// `kind` is the `ProposalChange` variant name; `tool` is `Some` only for the variants
/// that name one. `change` is the whole `ProposalChange` serialized as JSON (not just its
/// inner fields), per the Engine's documented contract.
fn to_push_payload(proposal: &ConfigProposal) -> ProposalPushPayload {
    let kind = match &proposal.change {
        ProposalChange::RaiseTimeout { .. } => "RaiseTimeout",
        ProposalChange::ReviewAllowlist { .. } => "ReviewAllowlist",
        ProposalChange::RaiseSpendCap { .. } => "RaiseSpendCap",
        ProposalChange::Other { .. } => "Other",
    };
    let tool = match &proposal.change {
        ProposalChange::RaiseTimeout { tool, .. }
        | ProposalChange::ReviewAllowlist { tool, .. } => Some(tool.clone()),
        ProposalChange::RaiseSpendCap { .. } | ProposalChange::Other { .. } => None,
    };
    ProposalPushPayload {
        id: proposal.id.to_string(),
        kind: kind.to_string(),
        tool,
        change: serde_json::to_value(&proposal.change).ok(),
        rationale: proposal.rationale.clone(),
        evidence_runs: proposal.source_run_ids.len(),
    }
}

/// Push every proposal currently in `store` to the Engine, if `YAATAL_ENGINE_URL` and
/// `YAATAL_ENGINE_TOKEN` are both set. No-op (debug log) otherwise. Never returns an
/// error: a push failure must not fail the run, only be visible in the runner's logs.
pub async fn sync_proposals_to_engine(store: &JsonlProposalStore) {
    let (url, token) = match (
        std::env::var("YAATAL_ENGINE_URL"),
        std::env::var("YAATAL_ENGINE_TOKEN"),
    ) {
        (Ok(url), Ok(token)) => (url, token),
        _ => {
            tracing::debug!(
                "YAATAL_ENGINE_URL/YAATAL_ENGINE_TOKEN not set; skipping proposal sync to engine"
            );
            return;
        }
    };

    let proposals = match store.read_all() {
        Ok(proposals) => proposals,
        Err(e) => {
            tracing::warn!("could not read proposal store for engine sync: {e}");
            return;
        }
    };
    if proposals.is_empty() {
        return;
    }

    let client = reqwest::Client::new();
    let endpoint = format!("{}/api/harness/proposals", url.trim_end_matches('/'));
    for proposal in &proposals {
        let payload = to_push_payload(proposal);
        match client
            .post(&endpoint)
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::debug!(proposal_id = %proposal.id, "pushed proposal to engine");
            }
            Ok(resp) => {
                tracing::warn!(
                    proposal_id = %proposal.id,
                    status = %resp.status(),
                    "engine rejected proposal push"
                );
            }
            Err(e) => {
                tracing::warn!(
                    proposal_id = %proposal.id,
                    error = %e,
                    "failed to push proposal to engine"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn maps_raise_timeout_to_the_engine_payload_shape() {
        let proposal = ConfigProposal::new(
            vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            ProposalChange::RaiseTimeout {
                tool: "yaatal".to_string(),
                from_ms: 5_000,
                to_ms: 10_000,
            },
            "'yaatal' timed out in each of the last 3 runs",
            vec!["evidence line".to_string()],
        );

        let payload = to_push_payload(&proposal);

        assert_eq!(payload.id, proposal.id.to_string());
        assert_eq!(payload.kind, "RaiseTimeout");
        assert_eq!(payload.tool.as_deref(), Some("yaatal"));
        assert_eq!(payload.rationale, proposal.rationale);
        assert_eq!(payload.evidence_runs, 3);

        // `change` is the whole ProposalChange serialized as JSON, not just its fields.
        let change = payload.change.clone().expect("change present");
        let expected = serde_json::to_value(&proposal.change).expect("serializes");
        assert_eq!(change, expected);
        assert_eq!(
            change,
            serde_json::json!({"RaiseTimeout": {"tool": "yaatal", "from_ms": 5000, "to_ms": 10000}})
        );

        // The body itself must serialize (this is what actually goes over the wire).
        let body = serde_json::to_string(&payload).expect("payload serializes");
        assert!(body.contains("\"kind\":\"RaiseTimeout\""));
    }

    #[test]
    fn other_and_spend_cap_variants_have_no_tool() {
        let other = ConfigProposal::new(
            vec![Uuid::new_v4()],
            ProposalChange::Other {
                key: "some.key".to_string(),
                value: "some-value".to_string(),
            },
            "other rationale",
            vec![],
        );
        let payload = to_push_payload(&other);
        assert_eq!(payload.kind, "Other");
        assert_eq!(payload.tool, None);
        assert_eq!(payload.evidence_runs, 1);

        let spend_cap = ConfigProposal::new(
            vec![Uuid::new_v4(), Uuid::new_v4()],
            ProposalChange::RaiseSpendCap { from: 1.0, to: 2.0 },
            "spend cap rationale",
            vec![],
        );
        let payload = to_push_payload(&spend_cap);
        assert_eq!(payload.kind, "RaiseSpendCap");
        assert_eq!(payload.tool, None);
        assert_eq!(payload.evidence_runs, 2);
    }

    #[test]
    fn review_allowlist_carries_its_tool() {
        let proposal = ConfigProposal::new(
            vec![Uuid::new_v4()],
            ProposalChange::ReviewAllowlist {
                tool: "rm".to_string(),
                denial_count: 3,
            },
            "rationale",
            vec![],
        );
        let payload = to_push_payload(&proposal);
        assert_eq!(payload.kind, "ReviewAllowlist");
        assert_eq!(payload.tool.as_deref(), Some("rm"));
    }
}
