//! Search harness pipeline.
//!
//! This crate implements a simple retrieval and ranking pipeline for search
//! use cases. It demonstrates how to wire together the core traits from
//! `yaatal-core` with model adapters and policy engines. Real logic
//! should replace the stubs in `pipeline()`.
//!
//! ## Modules
//!
//! - [`enricher`]: Tool-augmented candidate enrichment
//! - [`reranker`]: LLM-based reranking for candidate scoring
//! - [`difficulty`]: Difficulty-aware query classification for pipeline routing
//! - [`streaming`]: Real-time streaming search pipeline (Picovoice pattern)

pub mod difficulty;
pub mod enricher;
pub mod reranker;
pub mod streaming;

use tracing::info;
use yaatal_core::{
    Candidate, HarnessError, PolicyEngine, PolicyResult, Ranker, RequestContext, Retriever,
    ScoredCandidate,
};

/// A placeholder search pipeline. In the future this function will
/// orchestrate retrieval, ranking and policy stages. For now it
/// demonstrates the expected function signature and logging.
pub async fn pipeline(
    ctx: &RequestContext,
    retriever: &dyn Retriever,
    ranker: &dyn Ranker,
    policy: &dyn PolicyEngine,
    query: &str,
) -> Result<PolicyResult, HarnessError> {
    info!(request_id = %ctx.request_id, query = %query, "search_pipeline_start");
    // Retrieve candidates
    let candidates: Vec<Candidate> = retriever.retrieve(ctx, query).await?;
    info!(count = candidates.len(), "search_candidates");
    // Rank candidates
    let scored: Vec<ScoredCandidate> = ranker.rank(ctx, candidates).await?;
    info!(count = scored.len(), "search_ranked");
    // Apply policy
    let result: PolicyResult = policy.evaluate(ctx, scored).await?;
    info!(
        allowed = result.allowed.len(),
        denied = result.denied.len(),
        "search_policy"
    );
    Ok(result)
}
