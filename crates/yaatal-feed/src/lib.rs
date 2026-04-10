//! yaatal-feed — reusable feed ranking pipeline primitives.
//!
//! Architecture adapted from xai-org/x-algorithm (Apache-2.0).
//! This crate provides a default social timeline composition plus generic
//! ranking, filtering, and selection building blocks.
//!
//! Quick start:
//!   let pipeline = FeedBuilder::build(post_repo, discovery_repo, WeightConfig::default());
//!   let query = FeedQuery::new("user-123", "SN", 25);
//!   let result = pipeline.execute(query, "req-abc").await;
//!   // result.candidates -> ranked feed candidates

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
