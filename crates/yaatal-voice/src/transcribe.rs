use std::time::Instant;
use thiserror::Error;

/// Default HuggingFace Whisper model for cloud transcription.
const DEFAULT_HF_MODEL: &str = "openai/whisper-large-v3";

/// Default transcription API timeout (30 seconds — African latency tolerant).
const TRANSCRIPTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum TranscriptionError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("API error (HTTP {status}): {message}")]
    Api { status: u16, message: String },
    #[error("Model is loading (estimated {estimated_secs:.1}s)")]
    ModelLoading { estimated_secs: f64 },
    #[error("Transcription returned empty result")]
    EmptyResult,
}

/// Result of a successful transcription.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Wall-clock duration of the transcription call in milliseconds.
    pub duration_ms: u64,
    /// The model that was used.
    pub model: String,
}

/// The intelligent transcription router.
/// Decides whether to use cloud APIs or run the local `candle` Whisper model
/// based on network condition flags.
pub struct TranscriptionRouter;

impl TranscriptionRouter {
    /// Transcribes audio payload using the default HuggingFace Whisper model.
    /// If `offline` is true, forces the use of the local inference path.
    pub async fn transcribe(
        audio_payload: &[u8],
        offline: bool,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        Self::transcribe_with_model(audio_payload, offline, DEFAULT_HF_MODEL).await
    }

    /// Transcribes audio payload using a specified model.
    /// If `offline` is true, routes to local inference (currently unsupported).
    pub async fn transcribe_with_model(
        audio_payload: &[u8],
        offline: bool,
        model: &str,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let start = Instant::now();

        if offline {
            tracing::info!("Network offline or gated: routing to local Candle model");
            let result = Self::transcribe_local(audio_payload, model)?;
            Ok(TranscriptionResult {
                text: result,
                duration_ms: start.elapsed().as_millis() as u64,
                model: model.to_string(),
            })
        } else {
            tracing::info!(
                model = model,
                "Network stable: routing to Cloud Whisper API"
            );
            let text = Self::transcribe_cloud(audio_payload, model).await?;
            Ok(TranscriptionResult {
                text,
                duration_ms: start.elapsed().as_millis() as u64,
                model: model.to_string(),
            })
        }
    }

    fn transcribe_local(_audio_payload: &[u8], _model: &str) -> Result<String, TranscriptionError> {
        tracing::error!("Local candle inference is not supported on this build target");
        // TODO(#E7): Wire candle-whisper for on-device inference
        Err(TranscriptionError::Api {
            status: 0,
            message: "Local ML engine unavailable on this build target".to_string(),
        })
    }

    async fn transcribe_cloud(
        audio_payload: &[u8],
        model: &str,
    ) -> Result<String, TranscriptionError> {
        // TODO(#E6): Wire up cloud transcription API cascade (HF → Groq → Deepgram).
        // For now, call HuggingFace Inference API with proper error handling.

        let hf_token = std::env::var("HF_API_TOKEN").unwrap_or_default();
        if hf_token.is_empty() {
            tracing::warn!("HF_API_TOKEN not set — cloud transcription will fail");
            return Err(TranscriptionError::Api {
                status: 0,
                message: "HF_API_TOKEN environment variable not set".to_string(),
            });
        }

        let url = format!("https://api-inference.huggingface.co/models/{}", model);

        let client = reqwest::Client::builder()
            .timeout(TRANSCRIPTION_TIMEOUT)
            .build()?;

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", hf_token))
            .header("Content-Type", "audio/wav")
            .body(audio_payload.to_vec())
            .send()
            .await?;

        let status = resp.status().as_u16();

        // Handle HuggingFace 503 "model is loading" response
        if status == 503 {
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "Model loading"}));
            let estimated_secs = body
                .get("estimated_time")
                .and_then(|v| v.as_f64())
                .unwrap_or(30.0);
            tracing::warn!(
                model = model,
                estimated_secs = estimated_secs,
                "HuggingFace model is loading"
            );
            return Err(TranscriptionError::ModelLoading { estimated_secs });
        }

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TranscriptionError::Api {
                status,
                message: body,
            });
        }

        // Parse the response — HF returns { "text": "..." }
        let body: serde_json::Value = resp.json().await.map_err(|e| TranscriptionError::Api {
            status: 200,
            message: format!("Failed to parse response: {}", e),
        })?;

        let text = body
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if text.is_empty() {
            return Err(TranscriptionError::EmptyResult);
        }

        Ok(text)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn error_display_network() {
        let err = TranscriptionError::Api {
            status: 500,
            message: "Internal Server Error".to_string(),
        };
        assert!(format!("{err}").contains("500"));
    }

    #[test]
    fn error_display_model_loading() {
        let err = TranscriptionError::ModelLoading {
            estimated_secs: 42.5,
        };
        let msg = format!("{err}");
        assert!(msg.contains("42.5"));
        assert!(msg.contains("loading"));
    }

    #[test]
    fn error_display_empty_result() {
        let err = TranscriptionError::EmptyResult;
        assert!(format!("{err}").contains("empty"));
    }

    #[tokio::test]
    async fn transcribe_offline_returns_error() {
        let result = TranscriptionRouter::transcribe(b"fake-audio", true).await;
        assert!(result.is_err());
    }
}
