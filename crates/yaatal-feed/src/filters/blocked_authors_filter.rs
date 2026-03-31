use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;
use std::collections::HashSet;

pub struct BlockedAuthorsFilter;

#[async_trait]
impl Filter<FeedQuery, FeedCandidate> for BlockedAuthorsFilter {
    async fn filter(
        &self,
        query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<FilterBitmap, FeedError> {
        let excluded: HashSet<&str> = query
            .blocked_ids
            .iter()
            .chain(query.muted_ids.iter())
            .map(|s| s.as_str())
            .collect();
        let kept = candidates
            .iter()
            .map(|candidate| !excluded.contains(candidate.author_id.as_str()))
            .collect();
        Ok(FilterBitmap::from_keep_flags(kept))
    }

    fn name(&self) -> &'static str {
        "BlockedAuthorsFilter"
    }
}
