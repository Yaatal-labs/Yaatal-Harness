//! Default social timeline builder.
//!
//! Wires the crate's default source, filter, scorer, and selector stack into a
//! ready-to-run ranking pipeline.

use crate::filters::age_filter::AgeFilter;
use crate::filters::blocked_authors_filter::BlockedAuthorsFilter;
use crate::filters::dedup_filter::DedupFilter;
use crate::filters::seen_posts_filter::SeenPostsFilter;
use crate::filters::self_post_filter::SelfPostFilter;
use crate::pipeline::executor::{FeedPipeline, PipelineConfig};
use crate::pipeline::traits::*;
use crate::scorers::author_diversity_scorer::AuthorDiversityScorer;
use crate::scorers::recency_scorer::RecencyScorer;
use crate::scorers::weighted_scorer::WeightedScorer;
use crate::selectors::TopKSelector;
use crate::sources::discovery_source::{DiscoveryRepository, DiscoverySource};
use crate::sources::following_source::{FollowingSource, PostRepository};
use crate::types::*;
use crate::weights::WeightConfig;
use std::sync::Arc;

pub struct FeedBuilder;

impl FeedBuilder {
    /// Builds the crate's default social timeline pipeline.
    pub fn build(
        post_repo: Arc<dyn PostRepository>,
        discovery_repo: Arc<dyn DiscoveryRepository>,
        config: WeightConfig,
    ) -> FeedPipeline<FeedQuery, FeedCandidate> {
        // Sources (run in parallel)
        let sources: Vec<Box<dyn Source<FeedQuery, FeedCandidate>>> = vec![
            Box::new(FollowingSource::new(post_repo, config.max_post_age_hours)),
            Box::new(DiscoverySource::new(discovery_repo)),
        ];

        // Pre-scoring filters (run sequentially — order matters)
        let filters: Vec<Box<dyn Filter<FeedQuery, FeedCandidate>>> = vec![
            Box::new(DedupFilter),
            Box::new(AgeFilter::new(config.max_post_age_hours)),
            Box::new(SelfPostFilter),
            Box::new(SeenPostsFilter),
            Box::new(BlockedAuthorsFilter),
        ];

        // Scorers (run sequentially — order matters)
        //
        // Default scorer chain: recency baseline → weighted combination → diversity.
        let scorers: Vec<Box<dyn Scorer<FeedQuery, FeedCandidate>>> = vec![
            Box::new(RecencyScorer::default()),
            Box::new(WeightedScorer::new(config)),
            Box::new(AuthorDiversityScorer::new(config)),
        ];

        // Selector
        let selector: Box<dyn Selector<FeedQuery, FeedCandidate>> =
            Box::new(TopKSelector::new(config.default_result_size));

        // Post-selection filters (safety, visibility — add when needed)
        let post_selection_filters: Vec<Box<dyn Filter<FeedQuery, FeedCandidate>>> = vec![];

        // Side effects (analytics, caching — add PostHog here)
        let critical_side_effects: Vec<Box<dyn CriticalSideEffect<FeedQuery, FeedCandidate>>> =
            vec![];
        let best_effort_side_effects: Arc<
            Vec<Box<dyn BestEffortSideEffect<FeedQuery, FeedCandidate>>>,
        > = Arc::new(vec![]);

        FeedPipeline {
            query_hydrators: vec![],
            sources,
            hydrators: vec![],
            filters,
            scorers,
            selector,
            post_selection_filters,
            critical_side_effects,
            best_effort_side_effects,
            config: PipelineConfig::default(),
            result_size: config.default_result_size,
        }
    }
}
