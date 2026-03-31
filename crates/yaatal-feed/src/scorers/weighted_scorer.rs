// yaatal-feed/src/scorers/weighted_scorer.rs
//
// Direct adaptation of x-algorithm home-mixer/scorers/weighted_scorer.rs.
// Same pattern: score = Σ(weight × P(action)), with voice-first weights.

use crate::pipeline::traits::*;
use crate::types::*;
use crate::weights::WeightConfig;
use async_trait::async_trait;

#[derive(Default)]
pub struct WeightedScorer {
    pub config: WeightConfig,
}

impl WeightedScorer {
    pub fn new(config: WeightConfig) -> Self {
        Self { config }
    }

    fn apply(score: Option<f64>, weight: f64) -> f64 {
        score.unwrap_or(0.0) * weight
    }

    fn compute(&self, candidate: &FeedCandidate) -> f64 {
        let s = &candidate.scores;
        let voice_multiplier = self.voice_boost(candidate);
        let commerce_multiplier = self.commerce_boost(candidate);
        let config = &self.config;

        let combined = Self::apply(s.p_listen, config.listen_weight)
            + Self::apply(s.p_listen_full, config.listen_full_weight)
            + Self::apply(s.p_reply, config.reply_weight)
            + Self::apply(s.p_like, config.like_weight)
            + Self::apply(s.p_share, config.share_weight)
            + Self::apply(s.p_repost, config.repost_weight)
            + Self::apply(s.p_profile_click, config.profile_click_weight)
            + Self::apply(s.p_follow, config.follow_weight)
            // Commerce weights
            + Self::apply(s.p_add_to_cart, config.add_to_cart_weight)
            + Self::apply(s.p_purchase, config.purchase_weight)
            // Continuous
            + Self::apply(s.predicted_listen_pct, config.listen_pct_weight)
            // Negative
            + Self::apply(s.p_skip, config.skip_weight)
            + Self::apply(s.p_mute, config.mute_weight)
            + Self::apply(s.p_block, config.block_weight)
            + Self::apply(s.p_report, config.report_weight);

        self.offset(combined) * voice_multiplier * commerce_multiplier
    }

    /// Voice posts with sufficient duration get a boost.
    fn voice_boost(&self, candidate: &FeedCandidate) -> f64 {
        match (&candidate.content_type, candidate.voice_duration_ms) {
            (ContentType::Voice | ContentType::VoiceText, Some(ms))
                if ms > self.config.voice_min_duration_ms =>
            {
                self.config.voice_post_boost
            }
            _ => 1.0,
        }
    }

    /// Commerce listings get a boost depending on config.
    fn commerce_boost(&self, candidate: &FeedCandidate) -> f64 {
        match candidate.content_type {
            ContentType::ProductListing => self.config.commerce_listing_boost,
            _ => 1.0,
        }
    }

    /// Same offset logic as X — keeps negative scores sortable.
    fn offset(&self, combined: f64) -> f64 {
        if combined < 0.0 {
            (combined + self.config.negative_weights_sum()) / self.config.positive_weights_sum()
                * WeightConfig::negative_scores_offset()
        } else {
            combined + WeightConfig::negative_scores_offset()
        }
    }
}

#[async_trait]
impl Scorer<FeedQuery, FeedCandidate> for WeightedScorer {
    async fn score(
        &self,
        _query: &FeedQuery,
        candidates: &[FeedCandidate],
    ) -> Result<Vec<FeedCandidate>, FeedError> {
        let scored = candidates
            .iter()
            .map(|c| {
                let weighted = self.compute(c);
                FeedCandidate {
                    weighted_score: Some(weighted),
                    ..Default::default()
                }
            })
            .collect();
        Ok(scored)
    }

    fn update(&self, candidate: &mut FeedCandidate, scored: FeedCandidate) {
        candidate.weighted_score = scored.weighted_score;
    }

    fn name(&self) -> &'static str {
        "WeightedScorer"
    }
}
