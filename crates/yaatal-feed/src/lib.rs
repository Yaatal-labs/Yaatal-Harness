//! Feed ranking harness pipeline.
//!
//! This crate provides a framework for building recommendation
//! pipelines (e.g. social feeds or content suggestions). A typical
//! pipeline may include candidate generation, feature enrichment,
//! lightweight and heavyweight ranking, diversification and policy
//! filtering.

use tracing::info;
use yaatal_core::{
    Candidate, HarnessError, PolicyEngine, PolicyResult, Ranker, RequestContext, Retriever,
    ScoredCandidate,
};

/// A placeholder feed pipeline. Invoke candidate generation, ranking and
/// policy filtering in sequence.
pub async fn pipeline(
    ctx: &RequestContext,
    retriever: &dyn Retriever,
    ranker: &dyn Ranker,
    policy: &dyn PolicyEngine,
    user_id: &str,
) -> Result<PolicyResult, HarnessError> {
    info!(request_id = %ctx.request_id, user_id = %user_id, "feed_pipeline_start");
    // Candidate generation based on the user
    let query = format!("feed_for:{}", user_id);
    let candidates: Vec<Candidate> = retriever.retrieve(ctx, &query).await?;
    info!(count = candidates.len(), "feed_candidates");
    let scored: Vec<ScoredCandidate> = ranker.rank(ctx, candidates).await?;
    info!(count = scored.len(), "feed_ranked");
    let result: PolicyResult = policy.evaluate(ctx, scored).await?;
    info!(
        allowed = result.allowed.len(),
        denied = result.denied.len(),
        "feed_policy"
    );
    Ok(result)
}
