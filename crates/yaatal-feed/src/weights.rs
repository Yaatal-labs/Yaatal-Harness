//! Runtime configuration for feed ranking weights.
//!
//! These defaults describe a generic social timeline. Ingestion-related budgets
//! live in `types::IngestionConfig`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeightConfig {
    // ─── Positive Weights ──────────────────────────────────────────────
    pub listen_weight: f64,
    pub listen_full_weight: f64,
    pub reply_weight: f64,
    pub like_weight: f64,
    pub share_weight: f64,
    pub repost_weight: f64,
    pub profile_click_weight: f64,
    pub follow_weight: f64,

    // Commerce/Learning weights
    pub add_to_cart_weight: f64,
    pub purchase_weight: f64,

    // ─── Negative Weights ──────────────────────────────────────────────
    pub skip_weight: f64,
    pub mute_weight: f64,
    pub block_weight: f64,
    pub report_weight: f64,

    // ─── Continuous Weights ────────────────────────────────────────────
    pub listen_pct_weight: f64,

    // ─── Content Boosts ────────────────────────────────────────────────
    pub voice_post_boost: f64,
    pub voice_min_duration_ms: u32,

    pub commerce_listing_boost: f64,

    // ─── Diversity ─────────────────────────────────────────────────────
    pub author_diversity_decay: f64,
    pub author_diversity_floor: f64,
    pub language_diversity_min_ratio: f64,

    // ─── Discovery ─────────────────────────────────────────────────────
    pub oon_boost: f64,
    pub oon_max_ratio: f64,

    // ─── Pipeline ──────────────────────────────────────────────────────
    pub max_post_age_hours: u64,
    pub default_result_size: usize,
}

impl Default for WeightConfig {
    fn default() -> Self {
        Self::social_defaults()
    }
}

impl WeightConfig {
    /// Baseline weights for a generic social feed.
    pub fn social_defaults() -> Self {
        Self {
            listen_weight: 1.0,
            listen_full_weight: 2.0,
            reply_weight: 11.0,
            like_weight: 0.5,
            share_weight: 5.0,
            repost_weight: 3.0,
            profile_click_weight: 2.0,
            follow_weight: 8.0,

            add_to_cart_weight: 0.0,
            purchase_weight: 0.0,

            skip_weight: -0.5,
            mute_weight: -74.0,
            block_weight: -74.0,
            report_weight: -200.0,

            listen_pct_weight: 3.0,

            voice_post_boost: 1.5,
            voice_min_duration_ms: 2000,

            commerce_listing_boost: 1.0,

            author_diversity_decay: 0.5,
            author_diversity_floor: 0.1,
            language_diversity_min_ratio: 0.15,

            oon_boost: 1.2,
            oon_max_ratio: 0.4,

            max_post_age_hours: 72,
            default_result_size: 25,
        }
    }

    /// Baseline weights for a listing-heavy or commerce-heavy feed.
    pub fn commerce_defaults() -> Self {
        let mut config = Self::social_defaults();
        config.add_to_cart_weight = 10.0;
        config.purchase_weight = 25.0;
        config.commerce_listing_boost = 1.2;
        config.voice_post_boost = 1.0;
        config
    }

    /// Temporary compatibility wrapper for older app-specific call sites.
    pub fn yokk_defaults() -> Self {
        Self::social_defaults()
    }

    /// Temporary compatibility wrapper for older app-specific call sites.
    pub fn njooba_defaults() -> Self {
        Self::commerce_defaults()
    }

    /// Sum of all positive weights — used for score normalization.
    pub fn positive_weights_sum(&self) -> f64 {
        self.listen_weight
            + self.listen_full_weight
            + self.reply_weight
            + self.like_weight
            + self.share_weight
            + self.repost_weight
            + self.profile_click_weight
            + self.follow_weight
            + self.add_to_cart_weight
            + self.purchase_weight
            + self.listen_pct_weight
    }

    /// Sum of all negative weights (absolute values).
    pub fn negative_weights_sum(&self) -> f64 {
        -(self.skip_weight + self.mute_weight + self.block_weight + self.report_weight)
    }

    pub fn negative_scores_offset() -> f64 {
        0.001
    }
}
