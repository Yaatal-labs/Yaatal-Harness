// yaatal-feed/src/selectors/mod.rs

use async_trait::async_trait;

use crate::pipeline::traits::Selector;
use crate::types::*;

pub struct TopKSelector {
    pub k: usize,
}

impl TopKSelector {
    pub fn new(k: usize) -> Self {
        Self { k }
    }
}

impl Default for TopKSelector {
    fn default() -> Self {
        Self {
            k: 25, // Default replaced
        }
    }
}

#[async_trait]
impl Selector<FeedQuery, FeedCandidate> for TopKSelector {
    fn score(&self, candidate: &FeedCandidate) -> f64 {
        candidate
            .final_score
            .or(candidate.weighted_score)
            .unwrap_or(0.0)
    }

    fn size(&self) -> Option<usize> {
        Some(self.k)
    }

    async fn select(
        &self,
        query: &FeedQuery,
        candidates: Vec<FeedCandidate>,
    ) -> Vec<FeedCandidate> {
        <Self as Selector<FeedQuery, FeedCandidate>>::sort(self, candidates)
            .into_iter()
            .take(self.size().unwrap_or(query.limit))
            .collect()
    }

    fn name(&self) -> &'static str {
        "TopKSelector"
    }
}
