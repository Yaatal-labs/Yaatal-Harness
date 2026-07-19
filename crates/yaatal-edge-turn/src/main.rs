use std::fs::OpenOptions;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use yaatal_audit::{AuditStore, JsonlAuditStore};
use yaatal_edge_turn::{
    EdgeTurnRequest, EdgeTurnRunner, HttpEngineContextSource, MinimindHttpBackend,
    MockProposalBackend, ModelBackendKind, ProposalBackend,
};
use yaatal_policy::tool_policy::{ToolPolicy, ToolPolicyGate};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(response) => match serde_json::to_string(&response) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => fail(error.to_string()),
        },
        Err(error) => fail(error),
    }
}

async fn run() -> Result<yaatal_edge_turn::EdgeTurnResponse, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("cannot read stdin: {error}"))?;
    let request: EdgeTurnRequest =
        serde_json::from_str(&input).map_err(|error| format!("invalid request JSON: {error}"))?;

    let audit_path = preflight_audit_path(
        &std::env::var("YAATAL_EDGE_AUDIT_PATH")
            .map_err(|_| "YAATAL_EDGE_AUDIT_PATH is required".to_string())?,
    )?;

    let engine_url = std::env::var("YAATAL_ENGINE_URL")
        .map_err(|_| "YAATAL_ENGINE_URL is required".to_string())?;
    let token =
        std::env::var("YAATAL_TOKEN").map_err(|_| "YAATAL_TOKEN is required".to_string())?;
    let context_source = Arc::new(HttpEngineContextSource::new(engine_url, token)?);

    let backend: Arc<dyn ProposalBackend> = match request.model_backend {
        ModelBackendKind::Mock => Arc::new(MockProposalBackend::from_env()),
        ModelBackendKind::Minimind => {
            let url = std::env::var("MINIMIND_URL")
                .map_err(|_| "MINIMIND_URL is required for model_backend=minimind".to_string())?;
            Arc::new(MinimindHttpBackend::new(url)?)
        }
    };

    let store: Arc<dyn AuditStore> = Arc::new(JsonlAuditStore::new(audit_path));
    let policy: Arc<dyn ToolPolicy> = Arc::new(ToolPolicyGate::new(
        [
            "studio.update_price_overlay".to_string(),
            "studio.mark_sold_out_overlay".to_string(),
            "studio.switch_product".to_string(),
        ],
        None,
        store.clone(),
    ));

    EdgeTurnRunner::new(
        context_source,
        backend,
        store,
        policy,
        "studio:livestream-agent",
    )
    .execute(request)
    .await
    .map_err(|error| error.to_string())
}

fn preflight_audit_path(raw_path: &str) -> Result<PathBuf, String> {
    if raw_path.trim().is_empty() {
        return Err("YAATAL_EDGE_AUDIT_PATH must be nonempty".to_string());
    }
    let audit_path = PathBuf::from(raw_path);
    if let Some(parent) = audit_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create audit directory: {error}"))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .map_err(|error| format!("cannot open audit file for append: {error}"))?;
    Ok(audit_path)
}

fn fail(message: String) -> ExitCode {
    eprintln!("{}", serde_json::json!({ "error": message }));
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::preflight_audit_path;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("yaatal-edge-turn-{label}-{}", uuid::Uuid::new_v4()))
    }

    use std::path::PathBuf;

    #[test]
    fn audit_preflight_creates_missing_parent_and_file() {
        let root = temp_path("preflight-create");
        let path = root.join("nested").join("audit.jsonl");

        let checked = preflight_audit_path(path.to_str().expect("UTF-8 temp path"))
            .expect("preflight succeeds");

        assert_eq!(checked, path);
        assert!(path.is_file());
        std::fs::remove_dir_all(root).expect("temp tree removed");
    }

    #[test]
    fn audit_preflight_rejects_empty_and_directory_targets() {
        assert!(preflight_audit_path("  ").is_err());

        let directory = temp_path("preflight-directory");
        std::fs::create_dir_all(&directory).expect("directory target created");
        assert!(preflight_audit_path(directory.to_str().expect("UTF-8 temp path")).is_err());
        std::fs::remove_dir_all(directory).expect("temp directory removed");
    }
}
