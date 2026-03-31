use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;

pub struct AgeFilter {
    max_post_age_hours: u64,
}

impl AgeFilter {
    pub fn new(max_post_age_hours: u64) -> Self {
        Self { max_post_age_hours }
    }
}

#[async_trait]
impl Filter<FeedQuery, FeedCandidate> for AgeFilter {
    async fn filter(
        &self,
        _query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<FilterBitmap, FeedError> {
        let now = Utc::now();
        let max_age = chrono::Duration::hours(self.max_post_age_hours as i64);
        let kept = candidates
            .iter()
            .map(|candidate| {
                candidate
                    .created_at
                    .map(|created_at| (now - created_at) < max_age)
                    .unwrap_or(false)
            })
            .collect();
        Ok(FilterBitmap::from_keep_flags(kept))
    }

    fn name(&self) -> &'static str {
        "AgeFilter"
    }
}
