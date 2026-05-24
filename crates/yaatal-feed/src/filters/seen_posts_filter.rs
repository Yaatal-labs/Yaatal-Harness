use crate::pipeline::traits::*;
use crate::types::FeedQuery;
use async_trait::async_trait;
use std::collections::HashSet;

pub struct SeenPostsFilter;

#[async_trait]
impl<C> Filter<FeedQuery, C> for SeenPostsFilter
where
    C: Clone + Identifiable + Send + Sync + 'static,
{
    async fn filter(&self, query: &FeedQuery, candidates: &[C]) -> Result<FilterBitmap, FeedError> {
        let seen: HashSet<&str> = query
            .seen_post_ids
            .iter()
            .chain(query.served_post_ids.iter())
            .map(|s| s.as_str())
            .collect();

        let kept = candidates
            .iter()
            .map(|candidate| !seen.contains(candidate.id()))
            .collect();
        Ok(FilterBitmap::from_keep_flags(kept))
    }

    fn name(&self) -> &'static str {
        "SeenPostsFilter"
    }
}
