//! Speaker-aware policy engine.
//!
//! Policy that restricts access based on speaker identification.
//! Inspired by Picovoice's speaker-aware wake word pattern.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use yaatal_policy::speaker_aware::{SpeakerAwarePolicy, SpeakerDatabase};
//!
//! let speaker_db = SpeakerDatabase::new();
//! speaker_db.register("user1", embedding);
//!
//! let policy = SpeakerAwarePolicy::new(
//!     Arc::new(AllowAllPolicy),
//!     Arc::new(speaker_db),
//!     vec!["user1".to_string(), "user2".to_string()],
//! );
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use yaatal_core::{HarnessError, PolicyEngine, PolicyResult, RequestContext, ScoredCandidate};

/// Speaker database for storing speaker embeddings.
pub struct SpeakerDatabase {
    embeddings: HashMap<String, Vec<f32>>,
    threshold: f32,
}

impl SpeakerDatabase {
    /// Create a new speaker database.
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            threshold: 0.8,
        }
    }

    /// Set the similarity threshold for identification.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Register a speaker with their embedding.
    pub fn register(&mut self, speaker_id: impl Into<String>, embedding: Vec<f32>) {
        self.embeddings.insert(speaker_id.into(), embedding);
    }

    /// Identify a speaker from their embedding.
    pub fn identify(&self, embedding: &[f32]) -> Option<String> {
        self.embeddings
            .iter()
            .map(|(id, stored_emb)| (id.clone(), cosine_similarity(embedding, stored_emb)))
            .filter(|(_, similarity)| *similarity > self.threshold)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
    }

    /// Check if a speaker is registered.
    pub fn is_registered(&self, speaker_id: &str) -> bool {
        self.embeddings.contains_key(speaker_id)
    }

    /// Get all registered speaker IDs.
    pub fn registered_speakers(&self) -> Vec<String> {
        self.embeddings.keys().cloned().collect()
    }
}

impl Default for SpeakerDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Speaker-aware policy that checks speaker authorization.
pub struct SpeakerAwarePolicy {
    inner: Arc<dyn PolicyEngine>,
    _speaker_db: Arc<SpeakerDatabase>,
    allowed_speakers: HashMap<String, bool>,
    default_allow: bool,
}

impl SpeakerAwarePolicy {
    /// Create a new speaker-aware policy.
    pub fn new(
        inner: Arc<dyn PolicyEngine>,
        speaker_db: Arc<SpeakerDatabase>,
        allowed_speakers: Vec<String>,
    ) -> Self {
        let allowed = allowed_speakers.into_iter().map(|s| (s, true)).collect();

        Self {
            inner,
            _speaker_db: speaker_db,
            allowed_speakers: allowed,
            default_allow: false,
        }
    }

    /// Set whether to allow unregistered speakers (default: deny).
    pub fn with_default_allow(mut self, allow: bool) -> Self {
        self.default_allow = allow;
        self
    }

    /// Check if a speaker is allowed.
    fn is_allowed_speaker(&self, speaker_id: &str) -> bool {
        self.allowed_speakers
            .get(speaker_id)
            .copied()
            .unwrap_or(self.default_allow)
    }

    /// Add an allowed speaker.
    pub fn add_allowed_speaker(&mut self, speaker_id: impl Into<String>) {
        self.allowed_speakers.insert(speaker_id.into(), true);
    }

    /// Remove an allowed speaker.
    pub fn remove_allowed_speaker(&mut self, speaker_id: &str) {
        self.allowed_speakers.insert(speaker_id.to_string(), false);
    }
}

#[async_trait]
impl PolicyEngine for SpeakerAwarePolicy {
    async fn evaluate(
        &self,
        ctx: &RequestContext,
        items: Vec<ScoredCandidate>,
    ) -> Result<PolicyResult, HarnessError> {
        // Check for speaker_id in context metadata
        let speaker_id = ctx.metadata.get("speaker_id");

        if let Some(speaker) = speaker_id {
            if !self.is_allowed_speaker(speaker) {
                warn!(speaker = %speaker, request_id = %ctx.request_id, "speaker_not_authorized");
                return Ok(PolicyResult {
                    allowed: vec![],
                    denied: items
                        .into_iter()
                        .map(|sc| (sc, format!("Speaker '{}' not authorized", speaker)))
                        .collect(),
                });
            }
            info!(speaker = %speaker, request_id = %ctx.request_id, "speaker_authorized");
        } else {
            // No speaker identified - check if we allow anonymous
            if !self.default_allow {
                warn!(request_id = %ctx.request_id, "no_speaker_identified");
                return Ok(PolicyResult {
                    allowed: vec![],
                    denied: items
                        .into_iter()
                        .map(|sc| (sc, "No speaker identified".to_string()))
                        .collect(),
                });
            }
        }

        // Delegate to inner policy
        self.inner.evaluate(ctx, items).await
    }
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/// Compute cosine similarity between two embeddings.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a = (a.iter().map(|x| x * x).sum::<f32>()).sqrt();
    let magnitude_b = (b.iter().map(|x| x * x).sum::<f32>()).sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

/// Create an embedding from audio samples (placeholder).
pub fn create_embedding_from_audio(audio: &[i16]) -> Vec<f32> {
    // Simple placeholder: use FFT-like approach for demo
    // Real implementation would use a proper embedding model
    let sample_count = audio.len().min(1600); // 100ms at 16kHz

    audio
        .iter()
        .take(sample_count)
        .enumerate()
        .map(|(i, sample)| {
            let freq = i as f32 * 2.0 * std::f32::consts::PI / sample_count as f32;
            (*sample as f32 / 32768.0) * freq.sin()
        })
        .collect()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AllowAllPolicy;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);
    }

    #[test]
    fn test_speaker_database_register() {
        let mut db = SpeakerDatabase::new();
        db.register("user1", vec![0.1, 0.2, 0.3]);

        assert!(db.is_registered("user1"));
        assert!(!db.is_registered("user2"));
    }

    #[test]
    fn test_speaker_database_identify() {
        let mut db = SpeakerDatabase::new();
        db.register("user1", vec![1.0, 0.0, 0.0]);

        // Exact match
        assert_eq!(db.identify(&[1.0, 0.0, 0.0]), Some("user1".to_string()));

        // No match for different embedding
        assert_eq!(db.identify(&[0.0, 1.0, 0.0]), None);
    }

    #[test]
    fn test_speaker_aware_policy_allowed() {
        let inner = Arc::new(AllowAllPolicy);
        let db = Arc::new(SpeakerDatabase::new());
        let policy = SpeakerAwarePolicy::new(inner, db, vec!["user1".to_string()]);

        let ctx = RequestContext::new("test");
        let mut ctx_with_speaker = ctx.clone();
        ctx_with_speaker
            .metadata
            .insert("speaker_id".to_string(), "user1".to_string());

        let items = vec![yaatal_core::ScoredCandidate {
            candidate: yaatal_core::Candidate {
                id: "doc1".to_string(),
                attributes: HashMap::new(),
            },
            score: 0.9,
            metadata: None,
        }];

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(policy.evaluate(&ctx_with_speaker, items.clone()));

        assert!(result.is_ok());
        let policy_result = result.unwrap();
        assert_eq!(policy_result.allowed.len(), 1);
    }

    #[test]
    fn test_speaker_aware_policy_denied() {
        let inner = Arc::new(AllowAllPolicy);
        let db = Arc::new(SpeakerDatabase::new());
        let policy = SpeakerAwarePolicy::new(inner, db, vec!["user1".to_string()]);

        let mut ctx = RequestContext::new("test");
        ctx.metadata
            .insert("speaker_id".to_string(), "unauthorized".to_string());

        let items = vec![yaatal_core::ScoredCandidate {
            candidate: yaatal_core::Candidate {
                id: "doc1".to_string(),
                attributes: HashMap::new(),
            },
            score: 0.9,
            metadata: None,
        }];

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(policy.evaluate(&ctx, items));

        assert!(result.is_ok());
        let policy_result = result.unwrap();
        assert_eq!(policy_result.allowed.len(), 0);
        assert_eq!(policy_result.denied.len(), 1);
    }

    #[test]
    fn test_allow_default() {
        let inner = Arc::new(AllowAllPolicy);
        let db = Arc::new(SpeakerDatabase::new());
        let policy = SpeakerAwarePolicy::new(inner, db, vec![]).with_default_allow(true);

        let ctx = RequestContext::new("test");

        let items = vec![yaatal_core::ScoredCandidate {
            candidate: yaatal_core::Candidate {
                id: "doc1".to_string(),
                attributes: HashMap::new(),
            },
            score: 0.9,
            metadata: None,
        }];

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(policy.evaluate(&ctx, items));

        assert!(result.is_ok());
        let policy_result = result.unwrap();
        assert_eq!(policy_result.allowed.len(), 1);
    }
}
