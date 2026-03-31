// yaatal-feed/src/scorers/recency_scorer.rs
//
// No equivalent in X — they rely entirely on the Grok transformer.
// YOKK needs this as the Day 1 scorer before ML engagement predictions exist.
// Simple exponential time decay: newer posts score higher.
// Replace with ML scorer (Bo AI predictions) when the model is trained.

use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;

pub struct RecencyScorer {
    /// Half-life in hours. After this many hours, recency score = 0.5.
    half_life_hours: f64,
}

impl Default for RecencyScorer {
    fn default() -> Self {
        Self {
            half_life_hours: 12.0,
        }
    }
}

impl RecencyScorer {
    fn time_decay(&self, age_hours: f64) -> f64 {
        // Exponential decay: score = 2^(-age / half_life)
        // At age 0: score = 1.0
        // At age = half_life: score = 0.5
        // At age = 2 * half_life: score = 0.25
        (2.0_f64).powf(-age_hours / self.half_life_hours)
    }
}

#[async_trait]
impl Scorer<FeedQuery, FeedCandidate> for RecencyScorer {
    async fn score(
        &self,
        _query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<Vec<FeedCandidate>, FeedError> {
        let now = Utc::now();
        let scored = candidates
            .iter()
            .map(|c| {
                let age_hours = c
                    .created_at
                    .map(|t| (now - t).num_minutes() as f64 / 60.0)
                    .unwrap_or(48.0); // unknown age defaults to 2 days old

                let recency = self.time_decay(age_hours);

                // Populate engagement scores with recency as a baseline
                // This gets combined by WeightedScorer downstream
                FeedCandidate {
                    scores: EngagementScores {
                        p_listen: Some(recency),
                        p_listen_full: Some(recency * 0.6), // assume 60% completion rate baseline
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .collect();
        Ok(scored)
    }

    fn update(&self, candidate: &mut FeedCandidate, scored: FeedCandidate) {
        // Only set scores that aren't already populated by a real ML scorer
        if candidate.scores.p_listen.is_none() {
            candidate.scores.p_listen = scored.scores.p_listen;
        }
        if candidate.scores.p_listen_full.is_none() {
            candidate.scores.p_listen_full = scored.scores.p_listen_full;
        }
    }

    fn name(&self) -> &'static str {
        "RecencyScorer"
    }
}
