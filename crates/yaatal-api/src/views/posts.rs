/// Post API response views.
use serde::{Deserialize, Serialize};

/// Response for a single post.
#[derive(Debug, Serialize, Deserialize)]
pub struct PostResponse {
    pub id: String,
    pub author_id: String,
    pub title: String,
    pub content: String,
    pub r#type: String,
    pub category: Option<String>,
    pub tags: Option<String>,
    pub upvotes: i32,
    pub comment_count: i32,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Paginated list of posts.
#[derive(Debug, Serialize, Deserialize)]
pub struct PostListResponse {
    pub posts: Vec<PostResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}
