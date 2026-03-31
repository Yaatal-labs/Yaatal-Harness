#![allow(clippy::unused_async)]
use axum::{body::Bytes, extract::State, routing::post};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use yaatal_voice::transcribe::TranscriptionRouter;

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

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/voice")
        .add("/transcribe", post(transcribe))
}
