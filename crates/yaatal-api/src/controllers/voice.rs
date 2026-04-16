#![allow(clippy::unused_async)]
use axum::{
    body::Bytes,
    extract::{ws::WebSocketUpgrade, State},
    response::Response as AxumResponse,
    routing::{get, post},
};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use yaatal_voice::transcribe::TranscriptionRouter;

use crate::services::voice_session::handle_voice_session;

#[derive(Serialize, Deserialize, Debug)]
pub struct TranscribeResponse {
    pub transcription: String,
}

pub async fn transcribe(State(_ctx): State<AppContext>, body: Bytes) -> Result<Response> {
    // Route the incoming audio bytes through the E6 transcription engine.
    // passing false defaults to Cloud routing when online, fallback to Candle when offline.
    let result = TranscriptionRouter::transcribe(&body, false)
        .await
        .map_err(|e| loco_rs::Error::string(&e.to_string()))?;

    format::json(TranscribeResponse {
        transcription: result.text,
    })
}

pub async fn session(
    ws: WebSocketUpgrade,
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> AxumResponse {
    let user_pid = auth.claims.pid.clone();
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
