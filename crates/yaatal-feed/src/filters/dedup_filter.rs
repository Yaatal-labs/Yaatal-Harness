use crate::pipeline::traits::*;
use async_trait::async_trait;
use std::collections::HashSet;

pub struct DedupFilter;

#[async_trait]
impl<Q, C> Filter<Q, C> for DedupFilter
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Identifiable + Send + Sync + 'static,
{
    async fn filter(&self, _query: &Q, candidates: &[C]) -> Result<FilterBitmap, FeedError> {
        let mut seen: HashSet<&str> = HashSet::with_capacity(candidates.len());
        let kept = candidates.iter().map(|c| seen.insert(c.id())).collect();

        Ok(FilterBitmap::from_keep_flags(kept))
    }

    fn name(&self) -> &'static str {
        "DedupFilter"
    }
}
