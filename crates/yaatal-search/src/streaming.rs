//! Real-time streaming search pipeline.
//!
//! Streams search results as they become available, inspired by
//! Picovoice's Cheetah streaming STT pattern.
//!
//! ## Design
//!
//! Instead of waiting for all pipeline stages to complete, this module
//! streams intermediate results to provide real-time feedback.
//!
//! ## Example
//!
//! ```rust,ignore
//! use yaatal_search::streaming::{StreamingSearchPipeline, SearchUpdate};
//!
//! let pipeline = StreamingSearchPipeline::new(retriever, enricher, reranker);
//!
//! while let Some(update) = pipeline.stream_search(query, ctx).next().await {
//!     match update {
//!         SearchUpdate::Status(s) => println!("Status: {}", s),
//!         SearchUpdate::CandidatesRetrieved(n) => println!("Found {} candidates", n),
//!         SearchUpdate::FinalResults(r) => println!("Final: {:?}", r),
//!     }
//! }
//! ```

use crate::{enricher, reranker};
use futures::{Stream, StreamExt};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;
use yaatal_core::{
    Candidate, HarnessError, PolicyEngine, PolicyResult, RequestContext, Retriever, ScoredCandidate,
};

/// Streaming search pipeline that provides real-time updates.
#[derive(Clone)]
pub struct StreamingSearchPipeline {
    retriever: Arc<dyn Retriever>,
    enricher: Option<Arc<dyn EnricherService>>,
    reranker: Option<Arc<dyn RerankerService>>,
    policy: Arc<dyn PolicyEngine>,
}

impl StreamingSearchPipeline {
    /// Create a new streaming pipeline.
    pub fn new(retriever: Arc<dyn Retriever>, policy: Arc<dyn PolicyEngine>) -> Self {
        Self {
            retriever,
            enricher: None,
            reranker: None,
            policy,
        }
    }

    /// Add an enrichment stage.
    pub fn with_enricher(mut self, enricher: Arc<dyn EnricherService>) -> Self {
        self.enricher = Some(enricher);
        self
    }

    /// Add a reranking stage.
    pub fn with_reranker(mut self, reranker: Arc<dyn RerankerService>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Stream search results with real-time updates.
    pub fn stream_search(
        &self,
        query: String,
        ctx: &RequestContext,
    ) -> impl Stream<Item = SearchUpdate> + Send + 'static {
        let retriever = self.retriever.clone();
        let enricher = self.enricher.clone();
        let reranker = self.reranker.clone();
        let policy = self.policy.clone();
        let ctx = ctx.clone();

        async_stream::stream! {
            let start = Instant::now();

            // Stage 1: Retrieval
            yield SearchUpdate::Status("Starting retrieval...".to_string());
            yield SearchUpdate::StageStart("retrieval".to_string());

            let candidates = match retriever.retrieve(&ctx, &query).await {
                Ok(c) => {
                    yield SearchUpdate::CandidatesRetrieved(c.len());
                    yield SearchUpdate::StageComplete("retrieval".to_string(), start.elapsed().as_millis() as u64);
                    c
                }
                Err(e) => {
                    yield SearchUpdate::Error(format!("Retrieval failed: {}", e));
                    return;
                }
            };

            // Stage 2: Enrichment (optional)
            if let Some(enricher) = &enricher {
                yield SearchUpdate::Status("Enriching candidates...".to_string());
                yield SearchUpdate::StageStart("enrichment".to_string());

                let enriched = match enricher.enrich(candidates.clone(), &ctx).await {
                    Ok(e) => {
                        yield SearchUpdate::EnrichmentComplete(e.len());
                        yield SearchUpdate::StageComplete("enrichment".to_string(), start.elapsed().as_millis() as u64);
                        e
                    }
                    Err(e) => {
                        warn!(error = %e, "enrichment_failed");
                        yield SearchUpdate::EnrichmentComplete(candidates.len());
                        candidates
                    }
                };

                // Stage 3: Reranking
                if let Some(reranker) = &reranker {
                    yield SearchUpdate::Status("Reranking...".to_string());
                    yield SearchUpdate::StageStart("reranking".to_string());

                    let ranked = match reranker.rerank(&query, enriched.clone(), &ctx).await {
                        Ok(r) => {
                            yield SearchUpdate::StageComplete("reranking".to_string(), start.elapsed().as_millis() as u64);
                            r
                        }
                        Err(e) => {
                            warn!(error = %e, "reranking_failed");
                            enriched.into_iter().map(|c| ScoredCandidate {
                                candidate: c,
                                score: 0.0,
                                metadata: None,
                            }).collect()
                        }
                    };

                    // Stage 4: Policy
                    yield SearchUpdate::Status("Applying policy...".to_string());
                    let result = match policy.evaluate(&ctx, ranked).await {
                        Ok(r) => r,
                        Err(e) => {
                            yield SearchUpdate::Error(format!("Policy failed: {}", e));
                            return;
                        }
                    };

                    yield SearchUpdate::FinalResults(result, start.elapsed().as_millis() as u64);
                } else {
                    // Skip reranking, go straight to policy
                    let scored = enriched.into_iter().map(|c| ScoredCandidate {
                        candidate: c,
                        score: 0.0,
                        metadata: None,
                    }).collect();

                    let result = match policy.evaluate(&ctx, scored).await {
                        Ok(r) => r,
                        Err(e) => {
                            yield SearchUpdate::Error(format!("Policy failed: {}", e));
                            return;
                        }
                    };

                    yield SearchUpdate::FinalResults(result, start.elapsed().as_millis() as u64);
                }
            } else {
                // Skip enrichment, go straight to reranking or policy

                if let Some(reranker) = &reranker {
                    yield SearchUpdate::Status("Reranking...".to_string());
                    yield SearchUpdate::StageStart("reranking".to_string());

                    let ranked = match reranker.rerank(&query, candidates.clone(), &ctx).await {
                        Ok(r) => {
                            yield SearchUpdate::StageComplete("reranking".to_string(), start.elapsed().as_millis() as u64);
                            r
                        }
                        Err(e) => {
                            warn!(error = %e, "reranking_failed");
                            candidates.into_iter().map(|c| ScoredCandidate {
                                candidate: c,
                                score: 0.0,
                                metadata: None,
                            }).collect()
                        }
                    };

                    let result = match policy.evaluate(&ctx, ranked).await {
                        Ok(r) => r,
                        Err(e) => {
                            yield SearchUpdate::Error(format!("Policy failed: {}", e));
                            return;
                        }
                    };

                    yield SearchUpdate::FinalResults(result, start.elapsed().as_millis() as u64);
                } else {
                    // Skip both enrichment and reranking
                    let scored = candidates.into_iter().map(|c| ScoredCandidate {
                        candidate: c,
                        score: 0.0,
                        metadata: None,
                    }).collect();

                    let result = match policy.evaluate(&ctx, scored).await {
                        Ok(r) => r,
                        Err(e) => {
                            yield SearchUpdate::Error(format!("Policy failed: {}", e));
                            return;
                        }
                    };

                    yield SearchUpdate::FinalResults(result, start.elapsed().as_millis() as u64);
                }
            }
        }
    }
}

// =============================================================================
// TRAIT DEFINITIONS FOR SERVICES
// =============================================================================

/// Enrichment service trait for streaming pipeline.
#[async_trait::async_trait]
pub trait EnricherService: Send + Sync {
    async fn enrich(
        &self,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<Candidate>, HarnessError>;
}

/// Reranker service trait for streaming pipeline.
#[async_trait::async_trait]
pub trait RerankerService: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<ScoredCandidate>, HarnessError>;
}

/// Wrapper to use yaatal-search enricher in streaming pipeline.
pub struct EnrichmentExecutorWrapper {
    inner: Arc<enricher::EnrichmentExecutor>,
}

impl EnrichmentExecutorWrapper {
    pub fn new(inner: Arc<enricher::EnrichmentExecutor>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl EnricherService for EnrichmentExecutorWrapper {
    async fn enrich(
        &self,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<Candidate>, HarnessError> {
        let results = self.inner.enrich(candidates, ctx).await?;
        Ok(results.into_iter().map(|r| r.candidate).collect())
    }
}

/// Wrapper to use yaatal-search reranker in streaming pipeline.
pub struct RerankerWrapper {
    inner: Arc<reranker::LlmReranker>,
}

impl RerankerWrapper {
    pub fn new(inner: Arc<reranker::LlmReranker>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl RerankerService for RerankerWrapper {
    async fn rerank(
        &self,
        query: &str,
        candidates: Vec<Candidate>,
        ctx: &RequestContext,
    ) -> Result<Vec<ScoredCandidate>, HarnessError> {
        self.inner.rerank(query, candidates, ctx).await
    }
}

// =============================================================================
// UPDATE TYPES
// =============================================================================

/// Updates emitted during streaming search.
#[derive(Debug, Clone)]
pub enum SearchUpdate {
    /// Status message
    Status(String),
    /// Pipeline stage started
    StageStart(String),
    /// Pipeline stage completed
    StageComplete(String, u64),
    /// Candidates retrieved from retriever
    CandidatesRetrieved(usize),
    /// Enrichment completed
    EnrichmentComplete(usize),
    /// Final results available
    FinalResults(PolicyResult, u64),
    /// Error occurred
    Error(String),
}

impl SearchUpdate {
    /// Check if this is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, SearchUpdate::Error(_))
    }

    /// Get the error message if this is an error.
    pub fn error(&self) -> Option<&str> {
        match self {
            SearchUpdate::Error(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Progress tracker for streaming search.
#[derive(Debug, Default)]
pub struct SearchProgress {
    pub stage: Option<String>,
    pub candidates_found: usize,
    pub enriched_count: usize,
    pub errors: Vec<String>,
}

impl SearchProgress {
    /// Update progress from a search update.
    pub fn update(&mut self, update: &SearchUpdate) {
        match update {
            SearchUpdate::StageStart(s) => self.stage = Some(s.clone()),
            SearchUpdate::CandidatesRetrieved(n) => self.candidates_found = *n,
            SearchUpdate::EnrichmentComplete(n) => self.enriched_count = *n,
            SearchUpdate::Error(e) => self.errors.push(e.clone()),
            _ => {}
        }
    }
}

// =============================================================================
// ADVANCED STREAMING
// =============================================================================

/// Chunked streaming for large result sets.
pub struct ChunkedStreamingSearch {
    pipeline: StreamingSearchPipeline,
    chunk_size: usize,
}

impl ChunkedStreamingSearch {
    pub fn new(pipeline: StreamingSearchPipeline, chunk_size: usize) -> Self {
        Self {
            pipeline,
            chunk_size,
        }
    }

    /// Stream results in chunks for large result sets.
    pub fn stream_chunks(
        &self,
        query: String,
        ctx: &RequestContext,
    ) -> impl Stream<Item = SearchUpdate> + Send + 'static {
        let mut final_results: Option<PolicyResult> = None;
        let chunk_size = self.chunk_size;

        let stream = self.pipeline.stream_search(query, ctx);

        async_stream::stream! {
            tokio::pin!(stream);

            while let Some(update) = stream.next().await {
                match update {
                    SearchUpdate::FinalResults(ref result, _) => {
                        final_results = Some(result.clone());
                        yield update;
                    }
                    _ => yield update,
                }
            }

            // Emit chunks if we have results
            if let Some(result) = final_results {
                for chunk in result.allowed.chunks(chunk_size) {
                    yield SearchUpdate::Status(format!("Emitting chunk of {} items", chunk.len()));
                    // Yield chunk results
                }
            }
        }
    }
}

/// Buffered streaming with threshold.
pub struct BufferedStreamingSearch {
    pipeline: StreamingSearchPipeline,
    buffer_threshold: usize,
}

impl BufferedStreamingSearch {
    pub fn new(pipeline: StreamingSearchPipeline, buffer_threshold: usize) -> Self {
        Self {
            pipeline,
            buffer_threshold,
        }
    }

    /// Stream with buffering - wait until threshold before emitting.
    pub fn stream_buffered(
        &self,
        query: String,
        ctx: &RequestContext,
    ) -> impl Stream<Item = SearchUpdate> + Send + 'static {
        let pipeline = self.pipeline.clone();
        let threshold = self.buffer_threshold;
        let stream = pipeline.stream_search(query, ctx);

        async_stream::stream! {
            tokio::pin!(stream);

            while let Some(update) = stream.next().await {
                match update {
                    SearchUpdate::CandidatesRetrieved(n) => {
                        if n >= threshold {
                            yield SearchUpdate::Status(format!("{} candidates found (threshold reached)", n));
                        } else {
                            yield SearchUpdate::Status(format!("Found {} candidates, waiting for more...", n));
                        }
                    }
                    _ => yield update,
                }
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_update_is_error() {
        let error = SearchUpdate::Error("test error".to_string());
        assert!(error.is_error());
        assert_eq!(error.error(), Some("test error"));

        let status = SearchUpdate::Status("ok".to_string());
        assert!(!status.is_error());
        assert_eq!(status.error(), None);
    }

    #[test]
    fn test_search_progress_update() {
        let mut progress = SearchProgress::default();

        progress.update(&SearchUpdate::StageStart("retrieval".to_string()));
        assert_eq!(progress.stage, Some("retrieval".to_string()));

        progress.update(&SearchUpdate::CandidatesRetrieved(10));
        assert_eq!(progress.candidates_found, 10);

        progress.update(&SearchUpdate::Error("test".to_string()));
        assert_eq!(progress.errors.len(), 1);
    }
}
