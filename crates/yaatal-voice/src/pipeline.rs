//! Voice pipeline implementation.
//!
//! Integrates speech-to-text, intent parsing, LLM, and text-to-speech
//! into a unified voice-first pipeline.

use std::sync::Arc;
use tracing::{debug, info};
use yaatal_core::{ChatParams, HarnessError, LlmProvider, Message, RequestContext};

use crate::traits::{IntentParser as VoiceIntentParser, SpeechToText, TextToSpeech, VoiceIntent};

/// Configuration for voice pipeline.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Whether to require wake word detection.
    pub require_wake_word: bool,
    /// Confidence threshold for intent.
    pub intent_confidence_threshold: f32,
    /// Maximum audio length in seconds.
    pub max_audio_duration_secs: u32,
    /// Whether to enable streaming transcription.
    pub enable_streaming: bool,
    /// Default language code.
    pub language: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            require_wake_word: false,
            intent_confidence_threshold: 0.7,
            max_audio_duration_secs: 60,
            enable_streaming: true,
            language: "en".to_string(),
        }
    }
}

/// Voice pipeline that orchestrates STT → Intent → LLM → TTS.
pub struct VoicePipeline {
    config: VoiceConfig,
    stt: Arc<dyn SpeechToText>,
    intent_parser: Arc<dyn VoiceIntentParser>,
    llm: Arc<dyn LlmProvider>,
    tts: Arc<dyn TextToSpeech>,
    /// Optional search handler.
    search_handler: Option<Arc<dyn VoiceSearchHandler>>,
}

impl VoicePipeline {
    /// Create a new voice pipeline.
    pub fn new(
        stt: Arc<dyn SpeechToText>,
        intent_parser: Arc<dyn VoiceIntentParser>,
        llm: Arc<dyn LlmProvider>,
        tts: Arc<dyn TextToSpeech>,
    ) -> Self {
        Self {
            config: VoiceConfig::default(),
            stt,
            intent_parser,
            llm,
            tts,
            search_handler: None,
        }
    }

    /// Set configuration.
    pub fn with_config(mut self, config: VoiceConfig) -> Self {
        self.config = config;
        self
    }

    /// Add search handler for search intents.
    pub fn with_search_handler(mut self, handler: Arc<dyn VoiceSearchHandler>) -> Self {
        self.search_handler = Some(handler);
        self
    }

    /// Process audio and return voice response.
    pub async fn process_audio(
        &self,
        ctx: &RequestContext,
        audio: &[i16],
    ) -> Result<VoicePipelineResult, HarnessError> {
        debug!(request_id = %ctx.request_id, audio_len = audio.len(), "voice_pipeline_start");

        // 1. Check wake word (if required)
        if self.config.require_wake_word {
            if !self
                .stt
                .detect_wake_word(audio)
                .await
                .map_err(|e| HarnessError::External(e.to_string()))?
            {
                return Ok(VoicePipelineResult {
                    text: None,
                    audio: None,
                    intent: None,
                    success: false,
                    error: Some("Wake word not detected".to_string()),
                });
            }
        }

        // 2. Speech-to-text
        let transcript = self
            .stt
            .process(audio)
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;
        info!(transcript = %transcript, "voice_stt_complete");

        // 3. Intent parsing
        let intent = self
            .intent_parser
            .parse(&transcript)
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;
        info!(action = %intent.action, confidence = intent.confidence, "voice_intent");

        // 4. Handle based on intent
        let response_text = if intent.confidence < self.config.intent_confidence_threshold {
            // Low confidence: ask for clarification via LLM
            self.handle_low_confidence(&transcript, ctx).await?
        } else {
            match intent.action.as_str() {
                "search" => self.handle_search(&intent, ctx).await?,
                "general" | "unknown" => {
                    // Fallback to LLM
                    self.handle_llm(&transcript, ctx).await?
                }
                _ => self.handle_llm(&transcript, ctx).await?,
            }
        };

        // 5. Text-to-speech (optional)
        let audio_response = if self.tts.sample_rate() > 0 {
            Some(
                self.tts
                    .synthesize(&response_text)
                    .await
                    .map_err(|e| HarnessError::External(e.to_string()))?,
            )
        } else {
            None
        };

        Ok(VoicePipelineResult {
            text: Some(response_text),
            audio: audio_response,
            intent: Some(intent),
            success: true,
            error: None,
        })
    }

    /// Handle search intent.
    async fn handle_search(
        &self,
        intent: &VoiceIntent,
        ctx: &RequestContext,
    ) -> Result<String, HarnessError> {
        let query = intent
            .get("query")
            .cloned()
            .unwrap_or_else(|| "".to_string());

        if let Some(handler) = &self.search_handler {
            let results = handler
                .search(&query, ctx)
                .await
                .map_err(|e| HarnessError::External(e.to_string()))?;

            if results.is_empty() {
                return Ok(format!(
                    "I couldn't find anything for '{}'. Try a different search.",
                    query
                ));
            }

            let top_result = &results[0];
            Ok(format!(
                "I found {} result{}: {}",
                results.len(),
                if results.len() == 1 { "" } else { "s" },
                top_result
            ))
        } else {
            // No search handler, use LLM
            let prompt = format!(
                "The user wants to search for: '{}'. Provide a helpful response.",
                query
            );
            self.call_llm(&prompt, ctx).await
        }
    }

    /// Handle with LLM directly.
    async fn handle_llm(
        &self,
        transcript: &str,
        ctx: &RequestContext,
    ) -> Result<String, HarnessError> {
        let prompt = format!(
            "The user said: '{}'. Provide a helpful, concise response.",
            transcript
        );
        self.call_llm(&prompt, ctx).await
    }

    /// Handle low confidence intent.
    async fn handle_low_confidence(
        &self,
        transcript: &str,
        ctx: &RequestContext,
    ) -> Result<String, HarnessError> {
        let prompt = format!(
            "The user said '{}' but I wasn't confident about their intent. Ask them to clarify in a friendly way.",
            transcript
        );
        self.call_llm(&prompt, ctx).await
    }

    /// Call LLM with prompt.
    async fn call_llm(&self, prompt: &str, ctx: &RequestContext) -> Result<String, HarnessError> {
        let messages = vec![
            Message::system(
                "You are a helpful voice assistant. Keep responses concise and friendly.",
            ),
            Message::user(prompt),
        ];

        let response = self
            .llm
            .chat(ctx, &messages, ChatParams::default())
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;

        Ok(response.content)
    }
}

/// Result of voice pipeline processing.
#[derive(Debug, Clone)]
pub struct VoicePipelineResult {
    /// Generated text response.
    pub text: Option<String>,
    /// Generated audio samples.
    pub audio: Option<Vec<i16>>,
    /// Detected intent.
    pub intent: Option<VoiceIntent>,
    /// Whether processing succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

impl VoicePipelineResult {
    /// Check if this is a successful result.
    pub fn is_success(&self) -> bool {
        self.success && self.text.is_some()
    }
}

// =============================================================================
// VOICE SEARCH HANDLER
// =============================================================================

/// Trait for voice search handlers.
#[async_trait::async_trait]
pub trait VoiceSearchHandler: Send + Sync {
    async fn search(&self, query: &str, ctx: &RequestContext) -> Result<Vec<String>, HarnessError>;
}

/// Simple search handler using retriever.
pub struct RetrieverSearchHandler {
    retriever: Arc<dyn yaatal_core::Retriever>,
    ranker: Arc<dyn yaatal_core::Ranker>,
}

impl RetrieverSearchHandler {
    pub fn new(
        retriever: Arc<dyn yaatal_core::Retriever>,
        ranker: Arc<dyn yaatal_core::Ranker>,
    ) -> Self {
        Self { retriever, ranker }
    }
}

#[async_trait::async_trait]
impl VoiceSearchHandler for RetrieverSearchHandler {
    async fn search(&self, query: &str, ctx: &RequestContext) -> Result<Vec<String>, HarnessError> {
        let candidates = self.retriever.retrieve(ctx, query).await?;
        let scored = self.ranker.rank(ctx, candidates).await?;

        let results = scored
            .into_iter()
            .take(3)
            .map(|sc| {
                sc.candidate
                    .attributes
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| sc.candidate.id.clone())
            })
            .collect();

        Ok(results)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_config_defaults() {
        let config = VoiceConfig::default();
        assert!(!config.require_wake_word);
        assert_eq!(config.intent_confidence_threshold, 0.7);
        assert_eq!(config.language, "en");
    }

    #[test]
    fn test_voice_pipeline_result() {
        let result = VoicePipelineResult {
            text: Some("Hello".to_string()),
            audio: None,
            intent: None,
            success: true,
            error: None,
        };

        assert!(result.is_success());

        let failed = VoicePipelineResult {
            text: None,
            audio: None,
            intent: None,
            success: false,
            error: Some("Failed".to_string()),
        };

        assert!(!failed.is_success());
    }
}
