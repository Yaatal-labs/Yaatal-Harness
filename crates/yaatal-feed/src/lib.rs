// yaatal-feed — YOKK's feed ranking pipeline
//
// Architecture adapted from xai-org/x-algorithm (Apache-2.0).
// Same composable pipeline pattern, voice-first scoring weights.
//
// Quick start:
//   let pipeline = YokkFeedPipeline::build(post_repo, discovery_repo);
//   let query = YokkFeedQuery::new("user-123", "SN", 25);
//   let result = pipeline.execute(query, "req-abc").await;
//   // result.candidates → ranked Vec<VoicePostCandidate>

pub mod builder;
pub mod filters;
pub mod hydrators;
pub mod pipeline;
pub mod scorers;
pub mod selectors;
pub mod sources;
pub mod types;
pub mod weights;

pub use builder::FeedBuilder;
pub use pipeline::executor::FeedPipeline;
pub use types::*;
