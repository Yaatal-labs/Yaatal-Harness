use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, EntityTrait};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;
use chrono::Utc as ChronoUtc;

use yaatal_core::models::post;

type HmacSha256 = Hmac<Sha256>;

// ── Signature verification ────────────────────────────────────────────────────

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Verifies x-n8n-signature header against HMAC-SHA256(secret, body).
/// Matches the Node implementation: createHmac('sha256', secret).update(body).digest('hex')
fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> bool {
    let sig = match headers.get("x-n8n-signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());

    constant_time_eq(sig.as_bytes(), expected.as_bytes())
}

// ── Content sanitization ──────────────────────────────────────────────────────

fn sanitize(s: &str) -> String {
    // Strip script tags and common XSS vectors — matches YOKK implementation.
    let re_script = regex::Regex::new(r"(?i)<script\b[^<]*(?:(?!</script>)<[^<]*)*</script>").unwrap();
    let re_on = regex::Regex::new(r#"(?i)on\w+\s*=\s*["'][^"']*["']"#).unwrap();
    let re_js = regex::Regex::new(r"(?i)javascript:").unwrap();
    let s = re_script.replace_all(s, "");
    let s = re_on.replace_all(&s, "");
    re_js.replace_all(&s, "").trim().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[..max].to_string()
    }
}

// ── Workflow handlers ─────────────────────────────────────────────────────────

fn parse_post_type(s: &str) -> post::PostType {
    match s {
        "question"  => post::PostType::Question,
        "tutorial"  => post::PostType::Tutorial,
        "showcase"  => post::PostType::Showcase,
        _           => post::PostType::Discussion,
    }
}

async fn insert_post(
    db: &sea_orm::DatabaseConnection,
    title: &str,
    content: &str,
    post_type: &str,
    category: Option<&str>,
    author_id: &str,
) -> bool {
    let now = ChronoUtc::now().to_rfc3339();
    let model = post::ActiveModel {
        id:            Set(Uuid::new_v4().to_string()),
        author_id:     Set(author_id.to_string()),
        title:         Set(title.to_string()),
        content:       Set(content.to_string()),
        r#type:        Set(parse_post_type(post_type)),
        category:      Set(category.map(|s| s.to_string())),
        tags:          Set(None),
        upvotes:       Set(0),
        comment_count: Set(0),
        is_pinned:     Set(0),
        created_at:    Set(now.clone()),
        updated_at:    Set(now),
        ..Default::default()
    };
    post::Entity::insert(model).exec(db).await.is_ok()
}

async fn handle_rss_feed(body: &Value, db: &sea_orm::DatabaseConnection, bot_id: &str) -> (StatusCode, Value) {
    let items = match body.get("items").and_then(|v| v.as_array()) {
        Some(a) => a.clone(),
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "Invalid data format"})),
    };

    let mut inserted = 0u32;
    for item in &items {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let content = item.get("content")
            .or_else(|| item.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if title.is_empty() || content.is_empty() { continue; }
        let title   = sanitize(&truncate(&title, 500));
        let content = sanitize(&truncate(&content, 5000));
        if title.is_empty() || content.is_empty() { continue; }

        let category = item.get("category").and_then(|v| v.as_str());

        if insert_post(db, &title, &content, "discussion", category, bot_id).await {
            inserted += 1;
        }
    }
    (StatusCode::OK, serde_json::json!({"success": true, "inserted": inserted}))
}

async fn handle_github_trending(body: &Value, db: &sea_orm::DatabaseConnection, bot_id: &str) -> (StatusCode, Value) {
    let repos = match body.get("repositories").and_then(|v| v.as_array()) {
        Some(a) => a.clone(),
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "Invalid data format"})),
    };

    let mut inserted = 0u32;
    for repo in &repos {
        let name  = repo.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let desc  = repo.get("description").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || desc.is_empty() { continue; }

        let stars = repo.get("stars").and_then(|v| v.as_i64()).unwrap_or(0);
        let title   = sanitize(&truncate(&format!("{name} - {desc}"), 500));
        let content = sanitize(&format!(
            "Trending Repository\n\n**{name}**\n\n{desc}\n\nStars: {stars}"
        ));

        if insert_post(db, &title, &content, "showcase", Some("devtools"), bot_id).await {
            inserted += 1;
        }
    }
    (StatusCode::OK, serde_json::json!({"success": true, "inserted": inserted}))
}

async fn handle_devto_sync(body: &Value, db: &sea_orm::DatabaseConnection, bot_id: &str) -> (StatusCode, Value) {
    let articles = match body.get("articles").and_then(|v| v.as_array()) {
        Some(a) => a.clone(),
        None => return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "Invalid data format"})),
    };

    let mut inserted = 0u32;
    for article in &articles {
        let title = article.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if title.is_empty() { continue; }

        let reactions = article.get("positive_reactions_count").and_then(|v| v.as_i64()).unwrap_or(0);
        let content_raw = article.get("description")
            .or_else(|| article.get("body_markdown"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars().take(500).collect::<String>();
        let title   = sanitize(&truncate(title, 500));
        let content = sanitize(&content_raw);

        if insert_post(db, &title, &content, "tutorial", Some("edtech"), bot_id).await {
            inserted += 1;
        }
    }
    (StatusCode::OK, serde_json::json!({"success": true, "inserted": inserted}))
}

async fn handle_content_post(body: &Value, db: &sea_orm::DatabaseConnection, bot_id: &str) -> (StatusCode, Value) {
    let title   = body.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();

    if title.is_empty() || content.is_empty() {
        return (StatusCode::BAD_REQUEST, serde_json::json!({"error": "title and content required"}));
    }
    let title   = sanitize(&truncate(&title, 500));
    let content = sanitize(&truncate(&content, 5000));
    let category = body.get("category").and_then(|v| v.as_str());

    if insert_post(db, &title, &content, "discussion", category, bot_id).await {
        (StatusCode::OK, serde_json::json!({"success": true}))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "Failed to create post"}))
    }
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// POST /api/webhooks/n8n — receives n8n workflow payloads.
///
/// Validates x-n8n-signature against N8N_WEBHOOK_SECRET, then routes by
/// x-workflow-type header (or body.workflowType). All inserts go directly into
/// the Engine's Postgres (same as Supabase — RLS bypassed at direct connection level).
pub async fn n8n(
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let secret = match std::env::var("N8N_WEBHOOK_SECRET") {
        Ok(s) => s,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Webhook not configured").into_response();
        }
    };

    if !verify_signature(&headers, &body, &secret) {
        tracing::warn!("n8n webhook: signature verification failed");
        return (StatusCode::UNAUTHORIZED, "Invalid webhook signature").into_response();
    }

    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let bot_id = std::env::var("N8N_BOT_USER_ID").unwrap_or_default();

    let workflow_type = headers
        .get("x-workflow-type")
        .and_then(|v| v.to_str().ok())
        .or_else(|| parsed.get("workflowType").and_then(|v| v.as_str()))
        .unwrap_or("unknown");

    let (status, resp) = match workflow_type {
        "rss-feed"        => handle_rss_feed(&parsed, &ctx.db, &bot_id).await,
        "github-trending" => handle_github_trending(&parsed, &ctx.db, &bot_id).await,
        "devto-sync"      => handle_devto_sync(&parsed, &ctx.db, &bot_id).await,
        "content-post"    => handle_content_post(&parsed, &ctx.db, &bot_id).await,
        _                 => (StatusCode::BAD_REQUEST, serde_json::json!({"error": "Unknown workflow type"})),
    };

    (status, axum::Json(resp)).into_response()
}

/// GET /api/webhooks/n8n — health check for n8n.
pub async fn n8n_health() -> impl IntoResponse {
    axum::Json(serde_json::json!({"status": "ok", "endpoint": "n8n-webhook"}))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/webhooks")
        .add("/n8n", post(n8n))
        .add("/n8n", get(n8n_health))
}
