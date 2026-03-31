//! Posts CRUD controller — JWT-authenticated post management.
//!
//! All write operations require Bearer auth. XP is awarded on post creation.

use axum::extract::Path;
use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, EntityTrait, PaginatorTrait, QueryOrder};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    services::{profile_identity, xp_service},
    views::posts::{PostListResponse, PostResponse},
};
use yaatal_core::gamification::xp::XpAction;
use yaatal_core::models::post;

/// Request body for creating a post.
#[derive(Debug, Deserialize)]
pub struct CreatePostParams {
    pub title: String,
    pub content: String,
    pub r#type: Option<String>,
    pub category: Option<String>,
    pub tags: Option<String>,
}

/// Request body for updating a post.
#[derive(Debug, Deserialize)]
pub struct UpdatePostParams {
    pub title: Option<String>,
    pub content: Option<String>,
    pub category: Option<String>,
    pub tags: Option<String>,
}

/// Query params for listing posts with pagination.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

fn to_response(m: &post::Model) -> PostResponse {
    PostResponse {
        id: m.id.clone(),
        author_id: m.author_id.clone(),
        title: m.title.clone(),
        content: m.content.clone(),
        r#type: format!("{:?}", m.r#type).to_lowercase(),
        category: m.category.clone(),
        tags: m.tags.clone(),
        upvotes: m.upvotes,
        comment_count: m.comment_count,
        is_pinned: m.is_pinned != 0,
        created_at: m.created_at.clone(),
        updated_at: Some(m.updated_at.clone()),
    }
}

/// POST /api/posts — create a new post. Awards 25 XP.
#[debug_handler]
async fn create_post(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(params): Json<CreatePostParams>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let post_type = params.r#type.as_deref().unwrap_or("discussion");

    let model = post::ActiveModel {
        id: Set(id.clone()),
        author_id: Set(profile_id),
        title: Set(params.title),
        content: Set(params.content),
        r#type: Set(parse_post_type(post_type)),
        category: Set(params.category),
        tags: Set(params.tags),
        upvotes: Set(0),
        comment_count: Set(0),
        is_pinned: Set(0),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    };

    let inserted = post::Entity::insert(model).exec(&ctx.db).await?;
    let created = post::Entity::find_by_id(inserted.last_insert_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Best-effort XP award — don't fail the request if XP update fails
    let _ =
        xp_service::award_xp_by_user_pid(&ctx.db, &auth.claims.pid, XpAction::PostArticle).await;

    format::json(to_response(&created))
}

/// GET /api/posts — list posts with pagination.
#[debug_handler]
async fn list_posts(
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let page = params.page.unwrap_or(1).max(1) - 1;
    let per_page = params.per_page.unwrap_or(20).min(100);

    let paginator = post::Entity::find()
        .order_by_desc(post::Column::CreatedAt)
        .paginate(&ctx.db, per_page);

    let total = paginator.num_items().await?;
    let posts = paginator.fetch_page(page).await?;

    format::json(PostListResponse {
        posts: posts.iter().map(to_response).collect(),
        total,
        page: page + 1,
        per_page,
    })
}

/// GET /api/posts/{id} — get a single post.
#[debug_handler]
async fn show_post(State(ctx): State<AppContext>, Path(id): Path<String>) -> Result<Response> {
    let post = post::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    format::json(to_response(&post))
}

/// PUT /api/posts/{id} — update a post (author only).
#[debug_handler]
async fn update_post(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    Json(params): Json<UpdatePostParams>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = post::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Only the author can update their own post
    if existing.author_id != profile_id {
        return Err(Error::Unauthorized("not the post author".into()));
    }

    let mut active: post::ActiveModel = existing.into();
    if let Some(title) = params.title {
        active.title = Set(title);
    }
    if let Some(content) = params.content {
        active.content = Set(content);
    }
    if let Some(category) = params.category {
        active.category = Set(Some(category));
    }
    if let Some(tags) = params.tags {
        active.tags = Set(Some(tags));
    }
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());

    let updated = post::Entity::update(active).exec(&ctx.db).await?;
    format::json(to_response(&updated))
}

/// DELETE /api/posts/{id} — delete a post (author only).
#[debug_handler]
async fn remove_post(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = post::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.author_id != profile_id {
        return Err(Error::Unauthorized("not the post author".into()));
    }

    post::Entity::delete_by_id(&id).exec(&ctx.db).await?;
    format::empty()
}

/// Register post routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/posts")
        .add("/", post(create_post))
        .add("/", get(list_posts))
        .add("/{id}", get(show_post))
        .add("/{id}", put(update_post))
        .add("/{id}", delete(remove_post))
}

/// Parse string into `PostType` enum, defaulting to `Discussion`.
fn parse_post_type(s: &str) -> post::PostType {
    match s {
        "question" => post::PostType::Question,
        "tutorial" => post::PostType::Tutorial,
        "showcase" => post::PostType::Showcase,
        _ => post::PostType::Discussion,
    }
}
