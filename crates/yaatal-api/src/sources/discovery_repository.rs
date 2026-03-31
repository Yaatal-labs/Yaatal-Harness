//! SeaORM implementation of DiscoveryRepository for discovery source.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loco_rs::prelude::*;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use yaatal_core::models::post;
use yaatal_feed::sources::discovery_source::DiscoveryRepository as FeedDiscoveryRepository;
use yaatal_feed::types::{ContentType, FeedCandidate};

/// SeaORM-backed discovery repository for feed pipeline.
///
/// Day 1 implementation: returns trending/popular posts.
/// Future: integrate with yaatal-search ColBERT or Bo AI recommendations.
pub struct SeaOrmDiscoveryRepository {
    db: DatabaseConnection,
}

impl SeaOrmDiscoveryRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FeedDiscoveryRepository for SeaOrmDiscoveryRepository {
    async fn get_discovery_candidates(
        &self,
        _user_id: &str,
        _language_codes: &[String],
        exclude_author_ids: &[String],
        limit: usize,
    ) -> Result<Vec<FeedCandidate>, String> {
        // Build query: exclude followed/blocked/muted authors
        let mut query = post::Entity::find();

        if !exclude_author_ids.is_empty() {
            query = query.filter(post::Column::AuthorId.is_not_in(exclude_author_ids));
        }

        // Order by engagement signals (upvotes + comment_count) as proxy for "trending"
        // Future: replace with ColBERT semantic search or ML ranking
        let posts = query
            .order_by_desc(post::Column::Upvotes)
            .order_by_desc(post::Column::CommentCount)
            .order_by_desc(post::Column::CreatedAt)
            .limit(limit as u64)
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to fetch discovery posts: {}", e))?;

        let candidates = posts.into_iter().map(|p| model_to_candidate(&p)).collect();

        Ok(candidates)
    }
}

/// Convert a post::Model to FeedCandidate.
fn model_to_candidate(post: &post::Model) -> FeedCandidate {
    let created_at = DateTime::parse_from_rfc3339(&post.created_at)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    // TODO: map PostType variants to distinct ContentType once voice/commerce types land
    let content_type = ContentType::Text;

    FeedCandidate {
        id: post.id.clone(),
        author_id: post.author_id.clone(),
        created_at,
        content_type,
        text: Some(format!("{}\n\n{}", post.title, post.content)),
        language: None,
        voice_url: None,
        voice_duration_ms: None,
        in_reply_to_id: None,
        repost_of_id: None,
        in_network: None,
        author_username: None,
        author_display_name: None,
        author_followers_count: None,
        author_is_verified: None,
        scores: Default::default(),
        weighted_score: None,
        final_score: None,
        source_name: None,
        media_urls: vec![],
    }
}
