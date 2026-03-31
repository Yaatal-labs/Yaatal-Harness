//! Feed controller — ranked feed pipeline for user timeline.
//!
//! Combines following source (posts from followed accounts) with
//! discovery source (trending/recommended posts) using yaatal-feed pipeline.

use axum::extract::Query;
use loco_rs::prelude::*;
use serde::Deserialize;
use std::sync::Arc;
use yaatal_feed::weights::WeightConfig;
use yaatal_feed::{FeedBuilder, FeedQuery};

use crate::{
    services::profile_identity,
    sources::{SeaOrmDiscoveryRepository, SeaOrmPostRepository},
};

/// Query parameters for feed requests.
#[derive(Debug, Deserialize)]
pub struct FeedParams {
    /// Page number (default: 1)
    pub page: Option<u64>,
    /// Items per page (default: 20, max: 100)
    pub per_page: Option<u64>,
    /// Country code for localization (default: "SN")
    pub country: Option<String>,
    /// Only show posts from followed accounts (default: false)
    pub following_only: Option<bool>,
}

/// Response wrapper for feed results.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FeedResponse {
    pub items: Vec<FeedItem>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub request_id: String,
}

/// Individual feed item in the response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeedItem {
    pub id: String,
    pub author_id: String,
    pub content_type: String,
    pub text: Option<String>,
    pub language: Option<String>,
    pub voice_duration_ms: Option<u32>,
    pub score: f64,
    pub source: String,
    pub created_at: Option<String>,
}

impl FeedItem {
    fn from_candidate(c: &yaatal_feed::types::FeedCandidate) -> Self {
        Self {
            id: c.id.clone(),
            author_id: c.author_id.clone(),
            content_type: format!("{:?}", c.content_type).to_lowercase(),
            text: c.text.clone(),
            language: c.language.clone(),
            voice_duration_ms: c.voice_duration_ms,
            score: c.final_score.or(c.weighted_score).unwrap_or(0.0),
            source: c.source_name.clone().unwrap_or_else(|| "unknown".into()),
            created_at: c.created_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

/// GET /api/feed — ranked feed for authenticated user.
#[debug_handler]
async fn get_feed(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(params): Query<FeedParams>,
) -> Result<Response> {
    // Resolve authenticated user's profile
    let profile = profile_identity::resolve_profile_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    // Build feed query
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let country = params.country.unwrap_or_else(|| "SN".into());
    let in_network_only = params.following_only.unwrap_or(false);

    let mut query = FeedQuery::new(&profile.id, &country, (per_page * page) as usize);
    query.in_network_only = in_network_only;

    // TODO: Hydrate query with user's following/blocked/muted lists
    // For now, empty lists mean no filtering
    query.following_ids = vec![];
    query.blocked_ids = vec![];
    query.muted_ids = vec![];

    // Build repositories
    let post_repo = Arc::new(SeaOrmPostRepository::new(ctx.db.clone()));
    let discovery_repo = Arc::new(SeaOrmDiscoveryRepository::new(ctx.db.clone()));

    // Build and execute pipeline
    let pipeline = FeedBuilder::build(post_repo, discovery_repo, WeightConfig::default());
    let result = pipeline.execute(query, &auth.claims.pid).await;

    // Convert candidates to response format
    let total = result.candidates.len() as u64;
    let items: Vec<FeedItem> = result
        .candidates
        .iter()
        .map(FeedItem::from_candidate)
        .collect();

    // Paginate results
    let start = ((page - 1) * per_page) as usize;
    let end = (start + per_page as usize).min(items.len());
    let paginated_items = if start < items.len() {
        items[start..end].to_vec()
    } else {
        vec![]
    };

    format::json(FeedResponse {
        items: paginated_items,
        total,
        page,
        per_page,
        request_id: result.query.request_id.clone(),
    })
}

/// Register feed routes.
pub fn routes() -> Routes {
    Routes::new().prefix("api/feed").add("/", get(get_feed))
}
