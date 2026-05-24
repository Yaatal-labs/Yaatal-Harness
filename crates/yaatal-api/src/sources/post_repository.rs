//! SeaORM implementation of PostRepository for following source.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loco_rs::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use yaatal_core::models::post;
use yaatal_feed::sources::following_source::PostRepository as FeedPostRepository;
use yaatal_feed::types::{ContentType, FeedCandidate};

/// SeaORM-backed post repository for feed pipeline.
pub struct SeaOrmPostRepository {
    db: DatabaseConnection,
}

impl SeaOrmPostRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FeedPostRepository for SeaOrmPostRepository {
    async fn get_posts_by_authors(
        &self,
        author_ids: &[String],
        limit: usize,
        max_age_hours: u64,
    ) -> Result<Vec<FeedCandidate>, String> {
        if author_ids.is_empty() {
            return Ok(vec![]);
        }

        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let posts = post::Entity::find()
            .filter(post::Column::AuthorId.is_in(author_ids))
            .filter(post::Column::CreatedAt.gt(&cutoff_str))
            .order_by_desc(post::Column::CreatedAt)
            .limit(limit as u64)
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to fetch posts: {}", e))?;

        let candidates = posts.into_iter().map(|p| model_to_candidate(&p)).collect();

        Ok(candidates)
    }
}

/// Convert a post::Model to FeedCandidate.
fn model_to_candidate(post: &post::Model) -> FeedCandidate {
    // Parse created_at string to DateTime
    let created_at = DateTime::parse_from_rfc3339(&post.created_at)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    // Determine content type based on post metadata
    // TODO: map PostType variants to distinct ContentType once voice/commerce types land
    let content_type = ContentType::Text;

    FeedCandidate {
        id: post.id.clone(),
        author_id: post.author_id.clone(),
        created_at,
        content_type,
        text: Some(format!("{}\n\n{}", post.title, post.content)),
        language: None, // Could be derived from tags or content analysis
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
