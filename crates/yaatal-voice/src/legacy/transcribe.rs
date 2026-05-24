//! HF-Whisper transcription client (legacy path — preserved for emergency revert).

use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub model: String,
    pub duration_ms: u64,
}

pub async fn transcribe_audio(
    client: &Client, api_key: &str, audio_bytes: &[u8],
) -> Result<TranscriptionResult, reqwest::Error> {
    let response = client
        .post("https://api-inference.huggingface.co/models/openai/whisper-small")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "audio/wav")
        .body(audio_bytes.to_vec())
        .send().await?;
    let response = response.error_for_status()?;
    let json: serde_json::Value = response.json().await?;
    let text = json["text"].as_str().unwrap_or("").to_string();
    Ok(TranscriptionResult { text, model: "openai/whisper-small".into(), duration_ms: 0 })
}
