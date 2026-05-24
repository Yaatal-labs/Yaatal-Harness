/// Comment API response views.
use serde::{Deserialize, Serialize};

/// Response for a single comment.
#[derive(Debug, Serialize, Deserialize)]
pub struct CommentResponse {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub parent_id: Option<String>,
    pub content: String,
    pub is_accepted: bool,
    pub upvotes: i32,
    pub voice_url: Option<String>,
    pub created_at: String,
}

/// List of comments for a post.
#[derive(Debug, Serialize, Deserialize)]
pub struct CommentListResponse {
    pub comments: Vec<CommentResponse>,
    pub total: u64,
}
