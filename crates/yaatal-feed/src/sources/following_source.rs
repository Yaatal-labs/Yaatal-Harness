// yaatal-feed/src/sources/following_source.rs
//
// X's Thunder: in-memory post store, Kafka ingestion, sub-ms lookups.
// YOKK's FollowingSource: Turso DB query, edge replicas, <10ms lookups.
// Same role — "give me recent posts from accounts this user follows."

use crate::pipeline::traits::*;
use crate::types::*;
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for the actual DB client — implement with Turso/libSQL.
#[async_trait]
pub trait PostRepository: Send + Sync {
    async fn get_posts_by_authors(
        &self,
        author_ids: &[String],
        limit: usize,
        max_age_hours: u64,
    ) -> Result<Vec<FeedCandidate>, String>;
}

pub struct FollowingSource {
    pub repo: Arc<dyn PostRepository>,
    pub max_age_hours: u64,
    pub per_source_limit: usize,
}

impl FollowingSource {
    pub fn new(repo: Arc<dyn PostRepository>, max_age_hours: u64) -> Self {
        Self {
            repo,
            max_age_hours,
            per_source_limit: 200,
        }
    }
}

#[async_trait]
impl Source<FeedQuery, FeedCandidate> for FollowingSource {
    fn enable(&self, query: &FeedQuery) -> bool {
        !query.following_ids.is_empty()
    }

    async fn candidates(&self, query: &FeedQuery) -> Result<Vec<FeedCandidate>, FeedError> {
        let mut posts = self
            .repo
            .get_posts_by_authors(
                &query.following_ids,
                self.per_source_limit,
                self.max_age_hours,
            )
            .await
            .map_err(|e| FeedError::new("Source", "FollowingSource", e))?;

        // Tag all as in-network
        for post in posts.iter_mut() {
            post.in_network = Some(true);
            post.source_name = Some("following".into());
        }
        Ok(posts)
    }

    fn name(&self) -> &'static str {
        "FollowingSource"
    }
}
