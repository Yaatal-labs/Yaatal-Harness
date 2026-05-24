#![allow(clippy::unused_async)]
use axum::{
    body::Bytes,
    extract::{ws::WebSocketUpgrade, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use yaatal_voice::transcribe::TranscriptionRouter;

use crate::services::voice_session::handle_voice_session;

#[derive(Serialize, Deserialize, Debug)]
pub struct TranscribeResponse {
    pub transcription: String,
}

#[derive(Deserialize)]
pub struct WsTokenParam {
    pub token: String,
}

pub async fn transcribe(State(_ctx): State<AppContext>, body: Bytes) -> Result<Response> {
    let result = TranscriptionRouter::transcribe(&body, false)
        .await
        .map_err(|e| loco_rs::Error::string(&e.to_string()))?;

    format::json(TranscribeResponse {
        transcription: result.text,
    })
}

/// WebSocket endpoint. Browsers can't set Authorization headers on WS upgrades,
/// so the JWT is accepted as a ?token= query parameter instead.
pub async fn session(
    ws: WebSocketUpgrade,
    Query(params): Query<WsTokenParam>,
    State(ctx): State<AppContext>,
) -> AxumResponse {
    let secret = match std::env::var("JWT_SECRET") {
        Ok(s) => s,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "JWT not configured").into_response(),
    };

    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let claims: serde_json::Value = match decode(&params.token, &key, &validation) {
        Ok(data) => data.claims,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid token").into_response(),
    };

    let user_pid = match claims.get("pid").and_then(|v| v.as_str()) {
        Some(pid) => pid.to_string(),
        None => return (StatusCode::UNAUTHORIZED, "Missing pid claim").into_response(),
    };

    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_voice_session(ctx, user_pid, socket).await {
            tracing::warn!(error = %error, "voice session closed with error");
        }
    })
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/voice")
        .add("/transcribe", post(transcribe))
        .add("/session", get(session))
}
