//! Voice and multimodal harness pipeline.
//!
//! Voice-first pipeline inspired by Picovoice's LLM Voice Assistant recipe.
//! Provides wake word detection, speech-to-text, intent parsing, LLM,
//! and text-to-speech in a unified pipeline.
//!
//! ## Pipeline Flow
//!
//! ```text
//! Audio → Wake Word → STT → Intent Parser → LLM → TTS → Response
//! ```
//!
//! ## Modules
//!
//! - [`pipeline`]: Voice pipeline with STT, intent, LLM, TTS integration
//! - [`traits`]: Speech-to-text, intent parsing, TTS traits

pub mod pipeline;
pub mod traits;

pub use pipeline::{VoiceConfig, VoicePipeline};
pub use traits::{IntentParser as VoiceIntentParser, SpeechToText, TextToSpeech};

use tracing::info;
use yaatal_core::{HarnessError, RequestContext};

/// Represents the result of processing a voice request.
#[derive(Debug, Clone)]
pub struct VoiceResponse {
    pub text: String,
    pub confidence: f32,
    pub intent: Option<String>,
}

impl VoiceResponse {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            confidence: 1.0,
            intent: None,
        }
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }
}

/// Process a voice utterance. The pipeline currently returns a fixed
/// response and should be extended with ASR, NLU and response generation
/// logic.
pub async fn process_voice(
    ctx: &RequestContext,
    utterance: &str,
) -> Result<VoiceResponse, HarnessError> {
    info!(request_id = %ctx.request_id, utterance = %utterance, "voice_pipeline_start");
    // TODO: integrate ASR and NLU components here.
    Ok(VoiceResponse {
        text: format!("Echo: {}", utterance),
        confidence: 1.0,
        intent: None,
    })
}
