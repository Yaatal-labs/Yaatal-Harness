//! Comments CRUD controller — JWT-authenticated, nested under posts.
//!
//! Awards 10 XP on comment creation. Supports threaded replies via `parent_id`.

use axum::extract::Path;
use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    services::{profile_identity, xp_service},
    views::comments::{CommentListResponse, CommentResponse},
};
use yaatal_core::gamification::xp::XpAction;
use yaatal_core::models::comments;

/// Request body for creating a comment.
#[derive(Debug, Deserialize)]
pub struct CreateCommentParams {
    pub content: String,
    pub parent_id: Option<String>,
    pub voice_url: Option<String>,
}

fn to_response(m: &comments::Model) -> CommentResponse {
    CommentResponse {
        id: m.id.clone(),
        post_id: m.post_id.clone(),
        author_id: m.author_id.clone(),
        parent_id: m.parent_id.clone(),
        content: m.content.clone(),
        is_accepted: m.is_accepted != 0,
        upvotes: m.upvotes,
        voice_url: m.voice_url.clone(),
        created_at: m.created_at.clone(),
    }
}

/// POST /api/posts/{post_id}/comments — create a comment. Awards 10 XP.
#[debug_handler]
async fn create_comment(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(post_id): Path<String>,
    Json(params): Json<CreateCommentParams>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let model = comments::ActiveModel {
        id: Set(id.clone()),
        post_id: Set(post_id),
        author_id: Set(profile_id),
        parent_id: Set(params.parent_id),
        content: Set(params.content),
        is_accepted: Set(0),
        upvotes: Set(0),
        voice_url: Set(params.voice_url),
        created_at: Set(now),
    };

    let inserted = comments::Entity::insert(model).exec(&ctx.db).await?;
    let created = comments::Entity::find_by_id(inserted.last_insert_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Best-effort XP award
    let _ = xp_service::award_xp_by_user_pid(&ctx.db, &auth.claims.pid, XpAction::Comment).await;

    format::json(to_response(&created))
}

/// GET /api/posts/{post_id}/comments — list comments for a post.
#[debug_handler]
async fn list_comments(
    State(ctx): State<AppContext>,
    Path(post_id): Path<String>,
) -> Result<Response> {
    let all = comments::Entity::find()
        .filter(comments::Column::PostId.eq(&post_id))
        .order_by_asc(comments::Column::CreatedAt)
        .all(&ctx.db)
        .await?;

    let total = all.len() as u64;
    format::json(CommentListResponse {
        comments: all.iter().map(to_response).collect(),
        total,
    })
}

/// DELETE /api/posts/{post_id}/comments/{id} — delete a comment (author only).
#[debug_handler]
async fn remove_comment(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path((_post_id, id)): Path<(String, String)>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = comments::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.author_id != profile_id {
        return Err(Error::Unauthorized("not the comment author".into()));
    }

    comments::Entity::delete_by_id(&id).exec(&ctx.db).await?;
    format::empty()
}

/// Register comment routes (nested under posts).
pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/posts/{post_id}/comments")
        .add("/", post(create_comment))
        .add("/", get(list_comments))
        .add("/{id}", delete(remove_comment))
}
