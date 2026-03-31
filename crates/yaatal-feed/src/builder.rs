// yaatal-feed/src/builder.rs
//
// YOKK's equivalent of x-algorithm home-mixer/candidate_pipeline/phoenix_candidate_pipeline.rs.
// Wires all components together: sources → filters → scorers → selector.
//
// Usage:
//   let pipeline = YokkFeedPipeline::build(post_repo, discovery_repo);
//   let result = pipeline.execute(query, "req-123").await;
//   // result.candidates is your ranked feed

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
    /// Build the complete feed pipeline.
    ///
    /// X's PhoenixCandidatePipeline requires ~10 client dependencies.
    /// YOKK needs 2: a post repository and a discovery repository.
    /// Both backed by Turso — same DB, different query patterns.
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
        // X's chain: Phoenix ML → Weighted → AuthorDiversity → OON
        // YOKK Day 1: Recency → Weighted → AuthorDiversity
        // YOKK Day N: Replace Recency with Bo AI ML scorer
        // Scorers (run sequentially — order matters)
        let scorers: Vec<Box<dyn Scorer<FeedQuery, FeedCandidate>>> = vec![
            Box::new(RecencyScorer::default()), // baseline engagement prediction
            Box::new(WeightedScorer::new(config)), // combine predictions into score
            Box::new(AuthorDiversityScorer::new(config)), // prevent feed domination
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
