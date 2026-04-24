//! LLM-based reranking for search candidates.
//!
//! This module provides reranking functionality that uses a language model
//! to score and reorder candidates based on relevance to the query.
//!
//! ## Design
//!
//! The reranker sends the query and candidate contents to an LLM which
//! assigns relevance scores. These scores are used to reorder the candidates.
//!
//! ## Example
//!
//! ```rust,ignore
//! use yaatal_models::reranker::LlmReranker;
//! use yaatal_models::MockProvider;
//!
//! let llm = Arc::new(MockProvider::new("Mock response"));
//! let reranker = LlmReranker::new(llm);
//! let ranked = reranker.rerank(query, candidates, ctx).await;
//! ```

use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};
use yaatal_core::{
    Candidate, ChatParams, HarnessError, LlmProvider, Message, RequestContext, ScoredCandidate,
};

/// Prompt template for LLM-based reranking.
const DEFAULT_RERANK_PROMPT: &str = r#"You are a relevance judge evaluating search candidates.

Given a query and a list of documents, score each document's relevance on a scale of 0.0 to 10.0.

Rules:
- Score 9-10: Highly relevant, directly answers the query
- Score 7-8: Relevant, contains useful information
- Score 5-6: Partially relevant, tangentially related
- Score 3-4: Weakly relevant, marginal connection
- Score 0-2: Not relevant, off-topic

Query: {query}

Documents:
{documents}

Respond with a JSON array of scores:

```json
[
  {{"id": "doc1", "score": 8.5, "reason": "directly answers the query"}},
  {{"id": "doc2", "score": 6.0, "reason": "tangentially related"}}
]
```
"#;

/// Configuration for the reranker.
#[derive(Debug, Clone)]
pub struct RerankConfig {
    /// Prompt template to use.
    pub prompt_template: String,
    /// Number of top candidates to return.
    pub top_k: usize,
    /// Temperature for LLM sampling (lower = more deterministic).
    pub temperature: f32,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Truncate candidate content to this many characters.
    pub max_content_length: usize,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            prompt_template: DEFAULT_RERANK_PROMPT.to_string(),
            top_k: 20,
            temperature: 0.0, // Deterministic scoring
            max_tokens: 2048,
            max_content_length: 500,
        }
    }
}

/// Response from the LLM reranker.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoreEntry {
    pub id: String,
    pub score: f32,
    #[serde(default)]
    pub reason: String,
}

/// LLM-based reranker.
pub struct LlmReranker {
    llm: Arc<dyn LlmProvider>,
    config: RerankConfig,
}

impl LlmReranker {
    /// Create a new LLM reranker.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            llm,
            config: RerankConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(llm: Arc<dyn LlmProvider>, config: RerankConfig) -> Self {
        Self { llm, config }
    }

    /// Rerank candidates based on relevance to the query.
    ///
    /// This method sends the query and candidate contents to the LLM,
    /// which returns relevance scores. The candidates are then reordered
    /// based on these scores.
    pub async fn rerank(
        &self,
        query: &str,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<ScoredCandidate>, HarnessError> {
        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let start = std::time::Instant::now();
        info!(
            query = %query,
            candidates = candidates.len(),
            "rerank_start"
        );

        // Build the prompt
        let prompt = self.build_prompt(query, &candidates);

        // Call the LLM
        let messages = vec![
            Message::system("You are a relevance judge evaluating search candidates."),
            Message::user(&prompt),
        ];

        let params = ChatParams {
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
            ..Default::default()
        };

        let response = self
            .llm
            .chat(ctx, &messages, params)
            .await
            .map_err(|e| HarnessError::External(format!("LLM error: {}", e)))?;

        // Parse the scores
        let scores = self.parse_scores(&response.content)?;

        // Apply scores to candidates
        let scored = self.apply_scores(candidates, scores);

        let elapsed = start.elapsed();
        info!(
            candidates = scored.len(),
            duration_ms = elapsed.as_millis(),
            "rerank_complete"
        );

        Ok(scored)
    }

    /// Build the reranking prompt.
    fn build_prompt(&self, query: &str, candidates: &[Candidate]) -> String {
        let documents: Vec<String> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let content = if c.attributes.get("content").map(|s| s.len()).unwrap_or(0)
                    > self.config.max_content_length
                {
                    format!(
                        "{}...",
                        &c.attributes.get("content").unwrap()[..self.config.max_content_length]
                    )
                } else {
                    c.attributes.get("content").cloned().unwrap_or_default()
                };

                format!("[{}] {}\nSource: {}", i + 1, content, c.id)
            })
            .collect();

        self.config
            .prompt_template
            .replace("{query}", query)
            .replace("{documents}", &documents.join("\n\n"))
    }

    /// Parse scores from LLM response.
    fn parse_scores(&self, content: &str) -> Result<Vec<ScoreEntry>, HarnessError> {
        // Try to extract JSON from the response
        let json_str = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Parse the JSON
        serde_json::from_str(json_str).map_err(|e| {
            warn!(error = %e, content = %content, "rerank_parse_failed");
            HarnessError::Other(format!("Failed to parse LLM response: {}", e))
        })
    }

    /// Apply parsed scores to candidates and sort.
    fn apply_scores(
        &self,
        candidates: Vec<Candidate>,
        scores: Vec<ScoreEntry>,
    ) -> Vec<ScoredCandidate> {
        // Create a map of id -> score
        let score_map: std::collections::HashMap<String, (f32, String)> = scores
            .into_iter()
            .map(|s| (s.id, (s.score, s.reason)))
            .collect();

        // Apply scores to candidates
        let mut scored: Vec<ScoredCandidate> = candidates
            .into_iter()
            .map(|c| {
                let (score, reason) = score_map
                    .get(&c.id)
                    .cloned()
                    .unwrap_or_else(|| (5.0, "No score provided".to_string())); // Default 5.0

                ScoredCandidate {
                    candidate: c,
                    score,
                    metadata: Some(std::collections::HashMap::from([(
                        "reason".to_string(),
                        reason,
                    )])),
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top_k
        scored.into_iter().take(self.config.top_k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaatal_models::MockProvider;

    #[tokio::test]
    async fn test_rerank_config_defaults() {
        let config = RerankConfig::default();
        assert_eq!(config.top_k, 20);
        assert_eq!(config.temperature, 0.0);
        assert!(config.prompt_template.contains("{query}"));
    }

    #[tokio::test]
    async fn test_parse_scores() {
        let llm = Arc::new(MockProvider::new(
            r#"[{"id": "doc1", "score": 8.5, "reason": "relevant"}, {"id": "doc2", "score": 6.0}]"#
                .to_string(),
        ));
        let reranker = LlmReranker::new(llm);

        let candidates = vec![
            Candidate {
                id: "doc1".to_string(),
                attributes: std::collections::HashMap::new(),
            },
            Candidate {
                id: "doc2".to_string(),
                attributes: std::collections::HashMap::new(),
            },
        ];

        let ctx = RequestContext::new("test");
        let result = reranker
            .rerank("test query", candidates, &ctx)
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].score, 8.5);
        assert_eq!(result[1].score, 6.0);
    }
}
