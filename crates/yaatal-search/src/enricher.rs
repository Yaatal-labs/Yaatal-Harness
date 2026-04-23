//! Tool-augmented enrichment for the search pipeline.
//!
//! This module provides functionality to enhance candidates with additional
//! metadata fetched via tools (e.g., web fetch for URL metadata, content
//! summaries via LLM).
//!
//! ## Design
//!
//! Enrichment is applied as an optional stage between retrieval and ranking.
//! It uses the tool executor to fetch additional data for candidates that
//! have URLs or other fetchable identifiers.
//!
//! ## Example
//!
//! ```rust,ignore
//! use yaatal_search::enricher::EnrichmentExecutor;
//! use yaatal_core::RequestContext;
//!
//! let enricher = EnrichmentExecutor::new(tool_executor, memory_store);
//! let enriched = enricher.enrich_candidates(candidates, &config).await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use yaatal_core::{Candidate, HarnessError, MemoryEntry, MemoryStore, MemoryType, RequestContext};
use yaatal_tools::ToolExecutor;

/// Configuration for enrichment behavior.
#[derive(Debug, Clone)]
pub struct EnrichConfig {
    /// Whether to fetch metadata for candidates with URLs.
    pub fetch_metadata: bool,
    /// Whether to generate LLM summaries for content.
    pub fetch_summaries: bool,
    /// Maximum number of candidates to enrich (to avoid rate limits).
    pub max_candidates_to_enrich: usize,
    /// Cache enrichment results.
    pub use_cache: bool,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self {
            fetch_metadata: true,
            fetch_summaries: false,
            max_candidates_to_enrich: 10,
            use_cache: true,
        }
    }
}

/// Result of enriching a single candidate.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentResult {
    pub candidate: Candidate,
    pub metadata: HashMap<String, String>,
    pub cached: bool,
}

/// Tool-augmented enrichment executor.
///
/// Enrichment fetches additional data for candidates using the tool executor.
/// This data can include metadata from URLs, content summaries, or other
/// external information that wasn't available at retrieval time.
pub struct EnrichmentExecutor {
    tool_executor: Arc<ToolExecutor>,
    memory: Option<Arc<dyn MemoryStore>>,
    config: EnrichConfig,
}

impl EnrichmentExecutor {
    /// Create a new enrichment executor.
    pub fn new(tool_executor: Arc<ToolExecutor>) -> Self {
        Self {
            tool_executor,
            memory: None,
            config: EnrichConfig::default(),
        }
    }

    /// Set the memory store for caching enrichment results.
    pub fn with_memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Set the enrichment configuration.
    pub fn with_config(mut self, config: EnrichConfig) -> Self {
        self.config = config;
        self
    }

    /// Enrich a list of candidates with additional metadata.
    ///
    /// Only candidates with URLs or other fetchable identifiers will be enriched.
    /// Results are cached if a memory store is configured.
    pub async fn enrich(
        &self,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<EnrichmentResult>, HarnessError> {
        let start = Instant::now();
        let limit = self.config.max_candidates_to_enrich;

        info!(
            total = candidates.len(),
            limit = limit,
            fetch_metadata = self.config.fetch_metadata,
            "enrichment_start"
        );

        let mut results = Vec::with_capacity(candidates.len());

        for (i, candidate) in candidates.into_iter().enumerate() {
            // Check if we've hit the enrichment limit
            if i >= limit {
                // Just pass through the remaining candidates without enrichment
                results.push(EnrichmentResult {
                    candidate,
                    metadata: HashMap::new(),
                    cached: false,
                });
                continue;
            }

            // Try to get cached enrichment
            if self.config.use_cache {
                if let Some(cached) = self.get_cached_enrichment(&candidate).await? {
                    info!(candidate_id = %candidate.id, "enrichment_cache_hit");
                    results.push(cached);
                    continue;
                }
            }

            // Perform enrichment
            let result = self.enrich_candidate(candidate, ctx).await?;

            // Cache the result
            if self.config.use_cache {
                self.cache_enrichment(&result).await?;
            }

            results.push(result);
        }

        let elapsed = start.elapsed();
        info!(
            candidates = results.len(),
            cached_count = results.iter().filter(|r| r.cached).count(),
            duration_ms = elapsed.as_millis(),
            "enrichment_complete"
        );

        Ok(results)
    }

    /// Enrich a single candidate.
    async fn enrich_candidate(
        &self,
        mut candidate: Candidate,
        ctx: &RequestContext,
    ) -> Result<EnrichmentResult, HarnessError> {
        let mut metadata = HashMap::new();

        // Extract URL from candidate attributes
        if let Some(url) = candidate.attributes.get("url") {
            if self.config.fetch_metadata {
                match self.fetch_url_metadata(url, ctx).await {
                    Ok(fetched_metadata) => {
                        metadata.extend(fetched_metadata);
                    }
                    Err(e) => {
                        warn!(url = %url, error = %e, "enrichment_fetch_failed");
                    }
                }
            }
        }

        // Add enrichment metadata to candidate
        for (key, value) in metadata.clone() {
            candidate
                .attributes
                .insert(format!("enriched_{}", key), value);
        }

        Ok(EnrichmentResult {
            candidate,
            metadata,
            cached: false,
        })
    }

    /// Fetch metadata for a URL using the web fetch tool.
    async fn fetch_url_metadata(
        &self,
        url: &str,
        ctx: &RequestContext,
    ) -> Result<HashMap<String, String>, HarnessError> {
        let args = serde_json::json!({
            "url": url,
            "fields": ["title", "description", "og:image", "author"]
        });

        let result = self
            .tool_executor
            .execute(ctx, "web_fetch", &args.to_string())
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;

        if !result.success {
            return Err(HarnessError::External(format!(
                "Failed to fetch URL metadata: {}",
                result.error.unwrap_or_default()
            )));
        }

        // Parse the metadata from the result
        self.parse_metadata_result(&result.content, url)
    }

    /// Parse metadata from web fetch result.
    fn parse_metadata_result(
        &self,
        content: &str,
        _url: &str,
    ) -> Result<HashMap<String, String>, HarnessError> {
        // Try to parse as JSON first
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
            let mut metadata = HashMap::new();

            if let Some(title) = parsed.get("title").and_then(|v| v.as_str()) {
                metadata.insert("title".to_string(), title.to_string());
            }
            if let Some(desc) = parsed.get("description").and_then(|v| v.as_str()) {
                metadata.insert("description".to_string(), desc.to_string());
            }
            if let Some(image) = parsed.get("og_image").and_then(|v| v.as_str()) {
                metadata.insert("og_image".to_string(), image.to_string());
            }
            if let Some(author) = parsed.get("author").and_then(|v| v.as_str()) {
                metadata.insert("author".to_string(), author.to_string());
            }

            return Ok(metadata);
        }

        // Fallback: try to extract from HTML content
        Ok(self.extract_from_html(content))
    }

    /// Simple HTML metadata extraction fallback.
    fn extract_from_html(&self, content: &str) -> HashMap<String, String> {
        let mut metadata = HashMap::new();

        // Extract <title>
        if let Some(start) = content.find("<title") {
            if let Some(end) = content[start..].find("</title>") {
                let title = &content[start..start + end];
                if let Some(gt) = title.find('>') {
                    metadata.insert("title".to_string(), title[gt + 1..].trim().to_string());
                }
            }
        }

        // Extract og:description from meta tags
        if let Some(start) = content.find("property=\"og:description\"") {
            if let Some(content_start) = content[start..].find("content=\"") {
                let start_pos = start + content_start + 8;
                if let Some(end) = content[start_pos..].find('"') {
                    metadata.insert(
                        "description".to_string(),
                        content[start_pos..start_pos + end].to_string(),
                    );
                }
            }
        }

        metadata
    }

    /// Get cached enrichment for a candidate.
    async fn get_cached_enrichment(
        &self,
        candidate: &Candidate,
    ) -> Result<Option<EnrichmentResult>, HarnessError> {
        let memory = match &self.memory {
            Some(m) => m,
            None => return Ok(None),
        };

        // Search for cached enrichment
        let entries = memory
            .search(&candidate.id, Some(MemoryType::Fact), 1)
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;

        if let Some(entry) = entries.first() {
            // Try to deserialize the cached candidate
            if let Ok(result) = serde_json::from_str::<EnrichmentResult>(&entry.content) {
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    /// Cache an enrichment result.
    async fn cache_enrichment(&self, result: &EnrichmentResult) -> Result<(), HarnessError> {
        let memory = match &self.memory {
            Some(m) => m,
            None => return Ok(()),
        };

        let entry = MemoryEntry {
            id: format!("enrichment_{}", result.candidate.id),
            content: serde_json::to_string(result)
                .map_err(|e| HarnessError::Other(e.to_string()))?,
            memory_type: MemoryType::Fact,
            created_at: chrono::Utc::now(),
            accessed_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        memory
            .store(entry)
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;

        Ok(())
    }
}

/// Extension trait for converting enrichment results back to candidates.
pub trait IntoCandidates {
    fn into_candidates(self) -> Vec<Candidate>;
}

impl IntoCandidates for Vec<EnrichmentResult> {
    fn into_candidates(self) -> Vec<Candidate> {
        self.into_iter().map(|r| r.candidate).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enrich_config_defaults() {
        let config = EnrichConfig::default();
        assert!(config.fetch_metadata);
        assert!(!config.fetch_summaries);
        assert_eq!(config.max_candidates_to_enrich, 10);
        assert!(config.use_cache);
    }

    #[tokio::test]
    async fn test_html_extraction() {
        let executor = EnrichmentExecutor::new(Arc::new(ToolExecutor::new()));
        let html = r#"<html><head><title>Test Page</title></head><body></body></html>"#;
        let metadata = executor.extract_from_html(html);
        assert_eq!(metadata.get("title"), Some(&"Test Page".to_string()));
    }
}
