//! Voice pipeline traits.
//!
//! Defines traits for speech-to-text, intent parsing, and text-to-speech
//! components that can be plugged into the voice pipeline.

use futures::Stream as FuturesStream;
use std::collections::HashMap;
use std::pin::Pin;

/// Wrapper for async streams.
pub type BoxStream<'a, T> = Pin<Box<dyn FuturesStream<Item = T> + Send + 'a>>;

/// Errors specific to voice processing.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Speech recognition failed: {0}")]
    RecognitionFailed(String),
    #[error("Intent parsing failed: {0}")]
    IntentParsingFailed(String),
    #[error("Text-to-speech failed: {0}")]
    SynthesisFailed(String),
    #[error("Wake word not detected")]
    WakeWordNotDetected,
    #[error("No audio input")]
    NoAudio,
}

/// Trait for speech-to-text services.
#[async_trait::async_trait]
pub trait SpeechToText: Send + Sync {
    /// Process audio samples and return transcription.
    async fn process(&self, audio: &[i16]) -> Result<String, VoiceError>;

    /// Process audio in streaming mode, returning partial transcriptions.
    async fn process_streaming(
        &self,
        audio_chunks: BoxStream<'static, Vec<i16>>,
    ) -> Result<String, VoiceError>;

    /// Check if wake word is detected in audio.
    async fn detect_wake_word(&self, audio: &[i16]) -> Result<bool, VoiceError> {
        let _ = audio;
        Ok(true) // Default: always detected (placeholder)
    }
}

/// Trait for voice intent parsing.
#[async_trait::async_trait]
pub trait IntentParser: Send + Sync {
    /// Parse transcript into structured intent.
    async fn parse(&self, transcript: &str) -> Result<VoiceIntent, VoiceError>;

    /// Get supported intent actions.
    fn supported_actions(&self) -> Vec<&'static str>;
}

/// A parsed voice intent with action and entities.
#[derive(Debug, Clone)]
pub struct VoiceIntent {
    /// The action to perform (e.g., "search", "navigate", "play").
    pub action: String,
    /// Named entities extracted from the transcript.
    pub entities: HashMap<String, String>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
}

impl VoiceIntent {
    pub fn new(action: impl Into<String>, confidence: f32) -> Self {
        Self {
            action: action.into(),
            entities: HashMap::new(),
            confidence,
        }
    }

    pub fn with_entity(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.entities.insert(name.into(), value.into());
        self
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.entities.get(name)
    }
}

/// Trait for text-to-speech services.
#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Synthesize text to audio samples.
    async fn synthesize(&self, text: &str) -> Result<Vec<i16>, VoiceError>;

    /// Get the sample rate for this TTS engine.
    fn sample_rate(&self) -> u32 {
        16000 // Default sample rate
    }

    /// Get supported voices.
    fn supported_voices(&self) -> Vec<&'static str> {
        vec!["default"]
    }
}

// =============================================================================
// IMPLEMENTATIONS
// =============================================================================

/// Mock STT for testing.
pub struct MockSpeechToText;

impl MockSpeechToText {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockSpeechToText {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SpeechToText for MockSpeechToText {
    async fn process(&self, audio: &[i16]) -> Result<String, VoiceError> {
        let _ = audio;
        // Mock: return predefined transcript
        Ok("Hello, how can I help you?".to_string())
    }

    async fn process_streaming(
        &self,
        _audio_chunks: BoxStream<'static, Vec<i16>>,
    ) -> Result<String, VoiceError> {
        Ok("Mock streaming transcription".to_string())
    }
}

/// Mock intent parser for testing.
pub struct MockIntentParser;

impl MockIntentParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockIntentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl IntentParser for MockIntentParser {
    async fn parse(&self, transcript: &str) -> Result<VoiceIntent, VoiceError> {
        let lower = transcript.to_lowercase();

        let action = if lower.contains("search") || lower.contains("find") {
            "search"
        } else if lower.contains("play") || lower.contains("music") {
            "play"
        } else if lower.contains("navigate") || lower.contains("go to") {
            "navigate"
        } else {
            "general"
        };

        Ok(VoiceIntent::new(action, 0.9).with_entity("transcript", transcript))
    }

    fn supported_actions(&self) -> Vec<&'static str> {
        vec!["search", "play", "navigate", "general"]
    }
}

/// Mock TTS for testing.
pub struct MockTextToSpeech;

impl MockTextToSpeech {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockTextToSpeech {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TextToSpeech for MockTextToSpeech {
    async fn synthesize(&self, text: &str) -> Result<Vec<i16>, VoiceError> {
        // Mock: return silence
        let _ = text;
        Ok(vec![0i16; 1600]) // 100ms of silence at 16kHz
    }

    fn sample_rate(&self) -> u32 {
        16000
    }

    fn supported_voices(&self) -> Vec<&'static str> {
        vec!["mock", "default"]
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_stt() {
        let stt = MockSpeechToText::new();
        let result = stt.process(&[0i16; 100]).await.unwrap();
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_intent_parser() {
        let parser = MockIntentParser::new();
        let intent = parser.parse("search for Rust tutorials").await.unwrap();
        assert_eq!(intent.action, "search");
        assert!(intent.confidence > 0.5);
    }

    #[tokio::test]
    async fn test_mock_tts() {
        let tts = MockTextToSpeech::new();
        let audio = tts.synthesize("Hello").await.unwrap();
        assert_eq!(audio.len(), 1600); // 100ms at 16kHz
    }

    #[test]
    fn test_voice_intent() {
        let intent = VoiceIntent::new("search", 0.95).with_entity("query", "Rust programming");

        assert_eq!(intent.action, "search");
        assert_eq!(intent.get("query"), Some(&"Rust programming".to_string()));
        assert!(intent.confidence > 0.9);
    }
}
