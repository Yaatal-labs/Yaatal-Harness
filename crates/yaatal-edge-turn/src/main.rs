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

    let audit_path = PathBuf::from(
        std::env::var("YAATAL_EDGE_AUDIT_PATH")
            .unwrap_or_else(|_| "data/edge-turn-audit.jsonl".to_string()),
    );
    if let Some(parent) = audit_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create audit directory: {error}"))?;
    }
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

fn fail(message: String) -> ExitCode {
    eprintln!("{}", serde_json::json!({ "error": message }));
    ExitCode::from(2)
}
