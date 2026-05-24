//! Out-of-network discovery source.
//!
//! Starts with trending or popular retrieval and can later be backed by richer
//! recommendation systems without changing the ranking pipeline contract.

use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for the discovery backend.
#[async_trait]
pub trait DiscoveryRepository: Send + Sync {
    async fn get_discovery_candidates(
        &self,
        user_id: &str,
        language_codes: &[String],
        exclude_author_ids: &[String],
        limit: usize,
    ) -> Result<Vec<FeedCandidate>, String>;
}

pub struct DiscoverySource {
    pub repo: Arc<dyn DiscoveryRepository>,
    pub per_source_limit: usize,
}

impl DiscoverySource {
    pub fn new(repo: Arc<dyn DiscoveryRepository>) -> Self {
        Self {
            repo,
            per_source_limit: 100,
        }
    }
}

#[async_trait]
impl Source<FeedQuery, FeedCandidate> for DiscoverySource {
    fn enable(&self, query: &FeedQuery) -> bool {
        !query.in_network_only
    }

    async fn candidates(&self, query: &FeedQuery) -> Result<Vec<FeedCandidate>, FeedError> {
        // Exclude followed + blocked + muted authors from discovery
        let exclude: Vec<String> = query
            .following_ids
            .iter()
            .chain(query.blocked_ids.iter())
            .chain(query.muted_ids.iter())
            .cloned()
            .collect();

        let mut posts = self
            .repo
            .get_discovery_candidates(
                &query.user_id,
                &query.language_codes,
                &exclude,
                self.per_source_limit,
            )
            .await
            .map_err(|e| FeedError::new("Source", "DiscoverySource", e))?;

        // Tag all as out-of-network
        for post in posts.iter_mut() {
            post.in_network = Some(false);
            post.source_name = Some("discovery".into());
        }
        Ok(posts)
    }

    fn name(&self) -> &'static str {
        "DiscoverySource"
    }
}
