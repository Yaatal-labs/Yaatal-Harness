//! Push L1 Harness proposals into the Engine review system of record.
//!
//! The runner writes `proposals.jsonl` locally. This module is the narrow bridge from
//! that append-only artifact to Engine's `/api/harness/proposals` endpoint. It keeps the
//! repos decoupled: Harness sends JSON over HTTP, and Engine owns review decisions.

use std::path::{Path, PathBuf};

use yaatal_audit::proposals::{ConfigProposal, JsonlProposalStore, ProposalChange, ProposalStatus};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EngineProposalIngest {
    pub id: String,
    pub kind: String,
    pub tool: Option<String>,
    pub change: serde_json::Value,
    pub rationale: String,
    pub evidence_runs: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PushSummary {
    pub file: PathBuf,
    pub read: usize,
    pub pushed: usize,
    pub skipped_non_proposed: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalPushError {
    #[error("proposal evidence run count {0} exceeds i32::MAX")]
    TooManyEvidenceRuns(usize),
    #[error("proposal JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("proposal store error: {0}")]
    Store(#[from] yaatal_audit::AuditError),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Engine rejected proposal push with HTTP {status}: {body}")]
    EngineStatus {
        status: reqwest::StatusCode,
        body: String,
    },
}

pub fn engine_endpoint(engine_url: &str) -> String {
    format!("{}/api/harness/proposals", engine_url.trim_end_matches('/'))
}

pub fn to_engine_ingest(
    proposal: &ConfigProposal,
) -> Result<Option<EngineProposalIngest>, ProposalPushError> {
    if proposal.status != ProposalStatus::Proposed {
        return Ok(None);
    }

    let evidence_runs = i32::try_from(proposal.source_run_ids.len())
        .map_err(|_| ProposalPushError::TooManyEvidenceRuns(proposal.source_run_ids.len()))?;

    Ok(Some(EngineProposalIngest {
        id: proposal.id.to_string(),
        kind: change_kind(&proposal.change).to_string(),
        tool: change_tool(&proposal.change),
        change: serde_json::to_value(&proposal.change)?,
        rationale: proposal.rationale.clone(),
        evidence_runs,
    }))
}

pub fn collect_ingest_bodies(
    proposals: &[ConfigProposal],
) -> Result<(Vec<EngineProposalIngest>, usize), ProposalPushError> {
    let mut bodies = Vec::new();
    let mut skipped_non_proposed = 0;
    for proposal in proposals {
        match to_engine_ingest(proposal)? {
            Some(body) => bodies.push(body),
            None => skipped_non_proposed += 1,
        }
    }
    Ok((bodies, skipped_non_proposed))
}

pub async fn push_file(
    file: impl AsRef<Path>,
    engine_url: &str,
    token: &str,
) -> Result<PushSummary, ProposalPushError> {
    let file = file.as_ref();
    let store = JsonlProposalStore::new(file);
    let proposals = store.read_all()?;
    let read = proposals.len();
    let (bodies, skipped_non_proposed) = collect_ingest_bodies(&proposals)?;

    let client = reqwest::Client::new();
    let endpoint = engine_endpoint(engine_url);
    for body in &bodies {
        let response = client
            .post(&endpoint)
            .bearer_auth(token)
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProposalPushError::EngineStatus { status, body });
        }
    }

    Ok(PushSummary {
        file: file.to_path_buf(),
        read,
        pushed: bodies.len(),
        skipped_non_proposed,
    })
}

/// End-of-run auto-sync wrapper around [`push_file`]: silent no-op unless
/// `YAATAL_ENGINE_URL` and `YAATAL_TOKEN` are both set (the runner must keep
/// working offline), and failures are logged, never propagated — a push
/// failure must not fail the run. Anything not pushed retries on the next run;
/// the Engine upserts by id and never reverts a decided proposal, so re-push
/// is always safe. Operators get the same path explicitly via the
/// `yaatal-proposals-push` bin.
pub async fn sync_after_run(file: impl AsRef<Path>) {
    let (Ok(engine_url), Ok(token)) = (
        std::env::var("YAATAL_ENGINE_URL"),
        std::env::var("YAATAL_TOKEN"),
    ) else {
        tracing::debug!("YAATAL_ENGINE_URL/YAATAL_TOKEN not set; skipping proposal sync to engine");
        return;
    };
    match push_file(file, &engine_url, &token).await {
        Ok(summary) => tracing::info!(
            pushed = summary.pushed,
            skipped_non_proposed = summary.skipped_non_proposed,
            "synced proposals to engine"
        ),
        Err(error) => tracing::warn!(%error, "proposal sync to engine failed; will retry next run"),
    }
}

fn change_kind(change: &ProposalChange) -> &'static str {
    match change {
        ProposalChange::RaiseTimeout { .. } => "RaiseTimeout",
        ProposalChange::ReviewAllowlist { .. } => "ReviewAllowlist",
        ProposalChange::RaiseSpendCap { .. } => "RaiseSpendCap",
        ProposalChange::Other { .. } => "Other",
    }
}

fn change_tool(change: &ProposalChange) -> Option<String> {
    match change {
        ProposalChange::RaiseTimeout { tool, .. }
        | ProposalChange::ReviewAllowlist { tool, .. } => Some(tool.clone()),
        ProposalChange::RaiseSpendCap { .. } | ProposalChange::Other { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use uuid::Uuid;

    fn proposed(change: ProposalChange) -> ConfigProposal {
        ConfigProposal::new(
            vec![Uuid::new_v4(), Uuid::new_v4()],
            change,
            "supporting rationale",
            vec!["line one".to_string(), "line two".to_string()],
        )
    }

    #[test]
    fn maps_raise_timeout_to_engine_ingest_body() {
        let proposal = proposed(ProposalChange::RaiseTimeout {
            tool: "yaatal".to_string(),
            from_ms: 30_000,
            to_ms: 60_000,
        });

        let body = to_engine_ingest(&proposal)
            .expect("maps")
            .expect("proposed proposal is sent");

        assert_eq!(body.id, proposal.id.to_string());
        assert_eq!(body.kind, "RaiseTimeout");
        assert_eq!(body.tool.as_deref(), Some("yaatal"));
        assert_eq!(body.rationale, "supporting rationale");
        assert_eq!(body.evidence_runs, 2);
        assert_eq!(
            body.change,
            serde_json::json!({
                "RaiseTimeout": {
                    "tool": "yaatal",
                    "from_ms": 30000,
                    "to_ms": 60000
                }
            })
        );
    }

    #[test]
    fn skips_non_proposed_proposals() {
        let mut accepted = proposed(ProposalChange::ReviewAllowlist {
            tool: "git".to_string(),
            denial_count: 3,
        });
        accepted.status = ProposalStatus::Approved;
        let rejected = {
            let mut p = proposed(ProposalChange::RaiseSpendCap { from: 1.0, to: 2.0 });
            p.status = ProposalStatus::Rejected;
            p
        };
        let proposed = proposed(ProposalChange::Other {
            key: "runner.timeout".to_string(),
            value: "60s".to_string(),
        });

        let (bodies, skipped) =
            collect_ingest_bodies(&[accepted, rejected, proposed]).expect("collects");

        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].kind, "Other");
        assert_eq!(skipped, 2);
    }

    #[test]
    fn trims_engine_url_before_endpoint_join() {
        assert_eq!(
            engine_endpoint("https://engine.example.test/"),
            "https://engine.example.test/api/harness/proposals"
        );
    }

    #[test]
    fn push_file_posts_proposed_payload_to_engine() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock engine");
        let addr = listener.local_addr().expect("mock engine addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buf = vec![0; 8192];
            let n = stream.read(&mut buf).expect("read request");
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .expect("write response");
            request
        });

        let path = std::env::temp_dir().join(format!("yaatal-push-{}.jsonl", Uuid::new_v4()));
        let store = JsonlProposalStore::new(&path);
        store
            .append(&proposed(ProposalChange::ReviewAllowlist {
                tool: "yaatal".to_string(),
                denial_count: 3,
            }))
            .expect("append proposal");

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let summary = runtime
            .block_on(push_file(&path, &format!("http://{addr}"), "test-token"))
            .expect("push succeeds");

        assert_eq!(summary.read, 1);
        assert_eq!(summary.pushed, 1);
        assert_eq!(summary.skipped_non_proposed, 0);

        let request = server.join().expect("server joins");
        assert!(request.starts_with("POST /api/harness/proposals HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"),
            "request carried bearer token: {request}"
        );
        assert!(request.contains("\"kind\":\"ReviewAllowlist\""));
        assert!(request.contains("\"tool\":\"yaatal\""));
        assert!(request.contains("\"evidence_runs\":2"));

        let _ = std::fs::remove_file(path);
    }
}
