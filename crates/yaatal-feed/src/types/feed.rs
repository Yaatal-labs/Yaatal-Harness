use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pipeline::traits::Identifiable;

/// Query object passed through the ranking pipeline.
#[derive(Clone, Debug, Default)]
pub struct FeedQuery {
    pub user_id: String,
    pub request_id: String,
    pub language_codes: Vec<String>,
    pub country_code: String,
    pub seen_post_ids: Vec<String>,
    pub served_post_ids: Vec<String>,
    pub in_network_only: bool,
    pub cursor: Option<String>,
    pub limit: usize,

    // Hydrated by the caller or query hydrators before ranking.
    pub following_ids: Vec<String>,
    pub blocked_ids: Vec<String>,
    pub muted_ids: Vec<String>,
    pub engagement_history: Vec<EngagementEvent>,
}

impl FeedQuery {
    pub fn new(user_id: impl Into<String>, country: impl Into<String>, limit: usize) -> Self {
        Self {
            user_id: user_id.into(),
            request_id: Uuid::new_v4().to_string(),
            country_code: country.into(),
            limit,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngagementEvent {
    pub item_id: String,
    pub action: EngagementAction,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EngagementAction {
    Listen,
    ListenFull,
    Reply,
    Share,
    Like,
    Skip,
    Mute,
    Block,
    Report,
    ProfileClick,
    Follow,
    AddToCart,
    Purchase,
    Enroll,
    Other(String),
}

/// A feed item after scoring and selection.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeedItem {
    pub candidate_id: String,
    pub source_name: Option<String>,
    pub rank: usize,
    pub score: Option<f64>,
}

/// Candidate item that flows through the ranking pipeline.
#[derive(Clone, Debug, Default)]
pub struct FeedCandidate {
    pub id: String,
    pub author_id: String,
    pub created_at: Option<DateTime<Utc>>,
    pub content_type: ContentType,
    pub text: Option<String>,
    pub media_urls: Vec<String>,
    pub language: Option<String>,
    pub voice_url: Option<String>,
    pub voice_duration_ms: Option<u32>,
    pub in_reply_to_id: Option<String>,
    pub repost_of_id: Option<String>,
    pub in_network: Option<bool>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub author_followers_count: Option<u32>,
    pub author_is_verified: Option<bool>,
    pub scores: EngagementScores,
    pub weighted_score: Option<f64>,
    pub final_score: Option<f64>,
    pub source_name: Option<String>,
}

impl Identifiable for FeedCandidate {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ContentType {
    #[default]
    Text,
    Voice,
    VoiceText,
    ProductListing,
    CourseModule,
    Repost,
}

/// Engagement probability predictions used by scorers.
#[derive(Clone, Debug, Default)]
pub struct EngagementScores {
    pub p_listen: Option<f64>,
    pub p_listen_full: Option<f64>,
    pub p_reply: Option<f64>,
    pub p_like: Option<f64>,
    pub p_share: Option<f64>,
    pub p_repost: Option<f64>,
    pub p_profile_click: Option<f64>,
    pub p_follow: Option<f64>,
    pub p_add_to_cart: Option<f64>,
    pub p_purchase: Option<f64>,
    pub p_skip: Option<f64>,
    pub p_mute: Option<f64>,
    pub p_block: Option<f64>,
    pub p_report: Option<f64>,
    pub predicted_listen_pct: Option<f64>,
}
