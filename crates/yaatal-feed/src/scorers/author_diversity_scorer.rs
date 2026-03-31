// yaatal-feed/src/scorers/author_diversity_scorer.rs
//
// Direct adaptation of x-algorithm home-mixer/scorers/author_diversity_scorer.rs.
// Same pattern: exponential decay for repeated authors in a single feed page.
// If author appears 3 times: 1st gets full score, 2nd × 0.5, 3rd × 0.25 (with floor).

use crate::pipeline::traits::*;
use crate::types::*;
use crate::weights::WeightConfig;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Default)]
pub struct AuthorDiversityScorer {
    config: WeightConfig,
}

impl AuthorDiversityScorer {
    pub fn new(config: WeightConfig) -> Self {
        Self { config }
    }

    fn multiplier(&self, position: usize) -> f64 {
        let floor = self.config.author_diversity_floor;
        let decay = self.config.author_diversity_decay;
        (1.0 - floor) * decay.powf(position as f64) + floor
    }
}

#[async_trait]
impl Scorer<FeedQuery, FeedCandidate> for AuthorDiversityScorer {
    async fn score(
        &self,
        _query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<Vec<FeedCandidate>, FeedError> {
        let mut author_counts: HashMap<String, usize> = HashMap::new();
        let mut scored = vec![FeedCandidate::default(); candidates.len()];

        // Sort by weighted_score desc (same as X), then apply diversity decay
        let mut ordered: Vec<(usize, &FeedCandidate)> = candidates.iter().enumerate().collect();
        ordered.sort_by(|(_, a), (_, b)| {
            let a_score = a.weighted_score.unwrap_or(f64::NEG_INFINITY);
            let b_score = b.weighted_score.unwrap_or(f64::NEG_INFINITY);
            b_score.partial_cmp(&a_score).unwrap_or(Ordering::Equal)
        });

        for (original_idx, candidate) in ordered {
            let entry = author_counts
                .entry(candidate.author_id.clone())
                .or_insert(0);
            let position = *entry;
            *entry += 1;

            let multiplier = self.multiplier(position);
            let adjusted = candidate.weighted_score.map(|s| s * multiplier);

            scored[original_idx] = FeedCandidate {
                final_score: adjusted,
                ..Default::default()
            };
        }

        Ok(scored)
    }

    fn update(&self, candidate: &mut FeedCandidate, scored: FeedCandidate) {
        candidate.final_score = scored.final_score;
    }

    fn name(&self) -> &'static str {
        "AuthorDiversityScorer"
    }
}
