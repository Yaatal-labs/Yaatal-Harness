use axum::{body::Body, extract::State, http::StatusCode, response::Response};
use futures_util::StreamExt;
use loco_rs::prelude::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub model: Option<String>,
}

/// POST /api/ai/chat — proxy Ollama streaming to the client.
///
/// Requires Authorization: Bearer <supabase_jwt>. Returns text/event-stream
/// forwarded directly from Ollama, so the client can consume standard SSE.
/// Reads OLLAMA_BASE_URL (default: http://localhost:11434).
pub async fn chat(
    auth: auth::JWT,
    State(_ctx): State<AppContext>,
    Json(req): Json<ChatRequest>,
) -> Result<Response> {
    let base_url = std::env::var("OLLAMA_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());

    let model = req
        .model
        .as_deref()
        .unwrap_or("qwen3:8b")
        .to_string();

    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });

    let client = reqwest::Client::new();
    let ollama_resp = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| loco_rs::Error::string(&format!("Ollama request failed: {e}")))?;

    if !ollama_resp.status().is_success() {
        let status = ollama_resp.status().as_u16();
        return Ok(Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from(format!("Ollama error: {status}")))
            .unwrap());
    }

    // Forward Ollama's SSE stream directly — zero copy, no buffering.
    let byte_stream = ollama_resp
        .bytes_stream()
        .map(|r| r.map_err(|e| format!("stream error: {e}")));

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(byte_stream))
        .map_err(|e| loco_rs::Error::string(&e.to_string()))?;

    tracing::debug!(user = %auth.claims.pid, model = %model, "ai chat stream started");
    Ok(response)
}

pub fn routes() -> Routes {
    Routes::new().prefix("/api/ai").add("/chat", post(chat))
}
