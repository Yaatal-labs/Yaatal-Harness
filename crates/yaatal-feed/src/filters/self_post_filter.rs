use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;

pub struct SelfPostFilter;

#[async_trait]
impl Filter<FeedQuery, FeedCandidate> for SelfPostFilter {
    async fn filter(
        &self,
        query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<FilterBitmap, FeedError> {
        let kept = candidates
            .iter()
            .map(|candidate| candidate.author_id != query.user_id)
            .collect();
        Ok(FilterBitmap::from_keep_flags(kept))
    }

    fn name(&self) -> &'static str {
        "SelfPostFilter"
    }
}
