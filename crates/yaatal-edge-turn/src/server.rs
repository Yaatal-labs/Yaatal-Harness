//! HTTP server layer for yaatal-edge-turn.
//!
//! Wraps the existing [`EdgeTurnRunner`] in an axum HTTP server so Studio can
//! POST proposals over HTTP instead of piping JSON via stdin.
//!
//! Endpoints:
//! - `POST /edge-turn` — accepts [`EdgeTurnRequest`] JSON, returns [`EdgeTurnResponse`] JSON
//! - `GET /health` — health check
//!
//! The server creates a single [`EdgeTurnRunner`] at startup and shares it
//! across requests. Engine context is fetched per-request via a context source
//! that degrades gracefully to an empty session when Engine is unreachable.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use reqwest::Client;
use serde::Serialize;
use tracing::{debug, info, warn};

use yaatal_audit::{AuditStore, JsonlAuditStore};
use yaatal_policy::tool_policy::{ToolPolicy, ToolPolicyGate};

use crate::{
    ContextSource, EdgeTurnError, EdgeTurnRequest, EdgeTurnResponse, EdgeTurnRunner,
    EngineContext, MockProposalBackend, MinimindHttpBackend, ProposalBackend, CONTRACT_VERSION,
};

/// Default port for the HTTP server.
pub const DEFAULT_EDGE_TURN_PORT: u16 = 8090;
/// Default Engine API URL.
pub const DEFAULT_ENGINE_API_URL: &str = "http://yaatal-engine:8080";

// ---------------------------------------------------------------------------
// Context source with graceful degradation
// ---------------------------------------------------------------------------

/// Context source that fetches from Engine, degrading to an empty session
/// when Engine is unreachable.
pub struct DegradableContextSource {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl DegradableContextSource {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client builds with valid defaults");
        Self {
            client,
            base_url,
            token,
        }
    }
}

#[async_trait]
impl ContextSource for DegradableContextSource {
    async fn current(&self) -> Result<EngineContext, String> {
        let url = format!("{}/api/live-sessions/current/products", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }

        match req.send().await {
            Ok(response) if response.status().is_success() => response
                .json::<EngineContext>()
                .await
                .map_err(|error| format!("engine parse error: {error}")),
            Ok(response) => {
                let status = response.status();
                warn!(status = %status, "Engine returned non-success, degrading to empty context");
                Ok(degraded_context())
            }
            Err(error) => {
                warn!(error = %error, "Engine unreachable, degrading to empty context");
                Ok(degraded_context())
            }
        }
    }
}

/// Build an empty (degraded) Engine context used when the Engine is unreachable.
fn degraded_context() -> EngineContext {
    EngineContext {
        session: crate::EngineSession {
            id: "degraded".to_string(),
            merchant_id: String::new(),
            product_id: None,
            status: "degraded".to_string(),
            started_at: String::new(),
            ended_at: None,
            created_at: String::new(),
            updated_at: None,
        },
        products: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Server configuration
// ---------------------------------------------------------------------------

/// Configuration for the edge-turn HTTP server, read from environment.
pub struct ServerConfig {
    pub port: u16,
    pub engine_api_url: String,
    pub engine_token: Option<String>,
    pub audit_path: String,
    pub minimind_url: Option<String>,
}

impl ServerConfig {
    /// Read configuration from environment variables, applying defaults.
    pub fn from_env() -> Self {
        let port = std::env::var("EDGE_TURN_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_EDGE_TURN_PORT);

        let engine_api_url =
            std::env::var("ENGINE_API_URL").unwrap_or_else(|_| DEFAULT_ENGINE_API_URL.to_string());

        let engine_token = std::env::var("YAATAL_TOKEN").ok();

        let audit_path = std::env::var("EDGE_TURN_AUDIT_PATH")
            .or_else(|_| std::env::var("YAATAL_EDGE_AUDIT_PATH"))
            .unwrap_or_else(|_| "edge-turn-audit.jsonl".to_string());

        let minimind_url = std::env::var("MINIMIND_URL").ok();

        Self {
            port,
            engine_api_url,
            engine_token,
            audit_path,
            minimind_url,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared state passed to every request handler.
#[derive(Clone)]
pub struct ServerState {
    /// Runner for mock-backend requests.
    mock_runner: Arc<EdgeTurnRunner>,
    /// Runner for minimind-backend requests (None if MINIMIND_URL not set).
    minimind_runner: Option<Arc<EdgeTurnRunner>>,
}

impl ServerState {
    /// Pick the runner for the given model backend.
    fn runner_for(&self, backend: crate::ModelBackendKind) -> &Arc<EdgeTurnRunner> {
        match backend {
            crate::ModelBackendKind::Minimind => self
                .minimind_runner
                .as_ref()
                .unwrap_or(&self.mock_runner),
            crate::ModelBackendKind::Mock => &self.mock_runner,
        }
    }
}

/// Build the shared server state from config.
fn build_state(config: &ServerConfig) -> Result<ServerState, String> {
    let audit_path = preflight_audit_path(&config.audit_path)?;

    let context_source = Arc::new(DegradableContextSource::new(
        &config.engine_api_url,
        config.engine_token.clone(),
    ));

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

    let mock_backend: Arc<dyn ProposalBackend> = Arc::new(MockProposalBackend::from_env());
    let mock_runner = Arc::new(EdgeTurnRunner::new(
        context_source.clone(),
        mock_backend,
        store.clone(),
        policy.clone(),
        "studio:livestream-agent",
    ));

    let minimind_runner = if let Some(ref url) = config.minimind_url {
        let backend = MinimindHttpBackend::new(url)?;
        Some(Arc::new(EdgeTurnRunner::new(
            context_source,
            Arc::new(backend),
            store,
            policy,
            "studio:livestream-agent",
        )))
    } else {
        None
    };

    Ok(ServerState {
        mock_runner,
        minimind_runner,
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /edge-turn
async fn edge_turn_handler(
    State(state): State<ServerState>,
    Json(request): Json<EdgeTurnRequest>,
) -> Result<Json<EdgeTurnResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(run_id = %request.run_id, "edge-turn request received");

    let runner = state.runner_for(request.model_backend);

    match runner.execute(request).await {
        Ok(response) => {
            debug!(decision = ?response.decision, "edge-turn complete");
            Ok(Json(response))
        }
        Err(EdgeTurnError::InvalidRequest(msg)) => {
            warn!(error = %msg, "invalid request");
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: msg }),
            ))
        }
        Err(error) => {
            let message = error.to_string();
            warn!(error = %message, "edge-turn failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: message }),
            ))
        }
    }
}

/// GET /health
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: CONTRACT_VERSION,
    })
}

// ---------------------------------------------------------------------------
// Router + serve
// ---------------------------------------------------------------------------

/// Build the axum router with all routes.
pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/edge-turn", post(edge_turn_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

/// Start the HTTP server.
pub async fn serve(config: ServerConfig) -> Result<(), String> {
    let port = config.port;

    // Initialize tracing (best-effort — ignores error if already initialized).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yaatal_edge_turn=info".into()),
        )
        .try_init();

    let state = build_state(&config)?;
    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!(addr = %addr, "edge-turn HTTP server starting");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("failed to bind {addr}: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {e}"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Preflight-check the audit file path — creates parent dir and file if needed.
fn preflight_audit_path(raw_path: &str) -> Result<PathBuf, String> {
    if raw_path.trim().is_empty() {
        return Err("audit path must be nonempty".to_string());
    }
    let audit_path = PathBuf::from(raw_path);
    if let Some(parent) = audit_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create audit directory: {e}"))?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .map_err(|e| format!("cannot open audit file for append: {e}"))?;
    Ok(audit_path)
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_context_has_no_products() {
        let ctx = degraded_context();
        assert!(ctx.products.is_empty());
        assert_eq!(ctx.session.status, "degraded");
    }

    #[test]
    fn server_config_defaults_apply_when_env_absent() {
        let orig_port = std::env::var("EDGE_TURN_PORT").ok();
        let orig_url = std::env::var("ENGINE_API_URL").ok();

        std::env::remove_var("EDGE_TURN_PORT");
        std::env::remove_var("ENGINE_API_URL");

        let config = ServerConfig::from_env();
        assert_eq!(config.port, DEFAULT_EDGE_TURN_PORT);
        assert_eq!(config.engine_api_url, DEFAULT_ENGINE_API_URL);

        if let Some(v) = orig_port {
            std::env::set_var("EDGE_TURN_PORT", v);
        }
        if let Some(v) = orig_url {
            std::env::set_var("ENGINE_API_URL", v);
        }
    }
}