//! Products CRUD controller — JWT-authenticated product management.

use axum::extract::Path;
use loco_rs::prelude::*;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    services::profile_identity,
    views::products::{ProductListResponse, ProductResponse},
};
use yaatal_core::models::product;

/// Request body for creating a product.
#[derive(Debug, Deserialize)]
pub struct CreateProductParams {
    pub name: String,
    pub description: Option<String>,
    pub price_cents: i32,
    pub discount_price_cents: Option<i32>,
    pub stock: i32,
    pub category: String,
    pub images: Option<String>,
}

/// Request body for updating a product.
#[derive(Debug, Deserialize)]
pub struct UpdateProductParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price_cents: Option<i32>,
    pub discount_price_cents: Option<Option<i32>>,
    pub stock: Option<i32>,
    pub category: Option<String>,
    pub images: Option<Option<String>>,
    pub is_active: Option<bool>,
}

/// Query params for listing products.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub category: Option<String>,
    pub merchant_id: Option<String>,
    pub active_only: Option<bool>,
    pub search: Option<String>,
}

fn to_response(m: &product::Model) -> ProductResponse {
    ProductResponse {
        id: m.id.clone(),
        merchant_id: m.merchant_id.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        price_cents: m.price_cents,
        discount_price_cents: m.discount_price_cents,
        stock: m.stock,
        category: m.category.clone(),
        images: m.images.clone(),
        is_active: m.is_active != 0,
        upvotes: m.upvotes,
        created_at: m.created_at.clone(),
        updated_at: Some(m.updated_at.clone()),
    }
}

/// POST /api/products — create a new product.
#[debug_handler]
async fn create_product(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(params): Json<CreateProductParams>,
) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let model = product::ActiveModel {
        id: Set(id.clone()),
        merchant_id: Set(merchant_id),
        name: Set(params.name),
        description: Set(params.description),
        price_cents: Set(params.price_cents),
        discount_price_cents: Set(params.discount_price_cents),
        stock: Set(params.stock),
        category: Set(params.category),
        images: Set(params.images),
        is_active: Set(1),
        upvotes: Set(0),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    };

    let inserted = product::Entity::insert(model).exec(&ctx.db).await?;
    let created = product::Entity::find_by_id(inserted.last_insert_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    format::json(to_response(&created))
}

/// GET /api/products — list products with filters.
#[debug_handler]
async fn list_products(
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let page = params.page.unwrap_or(1).max(1) - 1;
    let per_page = params.per_page.unwrap_or(20).min(100);

    let mut query = product::Entity::find();

    if let Some(category) = &params.category {
        query = query.filter(product::Column::Category.eq(category));
    }

    if let Some(merchant_id) = &params.merchant_id {
        query = query.filter(product::Column::MerchantId.eq(merchant_id));
    }

    if params.active_only.unwrap_or(true) {
        query = query.filter(product::Column::IsActive.eq(1));
    }

    if let Some(search) = &params.search {
        let pattern = format!("%{}%", search);
        query = query.filter(
            Condition::any()
                .add(product::Column::Name.like(&pattern))
                .add(product::Column::Description.like(&pattern)),
        );
    }

    let paginator = query
        .order_by_desc(product::Column::CreatedAt)
        .paginate(&ctx.db, per_page);

    let total = paginator.num_items().await?;
    let products = paginator.fetch_page(page).await?;

    format::json(ProductListResponse {
        products: products.iter().map(to_response).collect(),
        total,
        page: page + 1,
        per_page,
    })
}

/// GET /api/products/{id} — get a single product.
#[debug_handler]
async fn show_product(State(ctx): State<AppContext>, Path(id): Path<String>) -> Result<Response> {
    let product = product::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if product.is_active == 0 {
        return Err(Error::NotFound);
    }

    format::json(to_response(&product))
}

/// PUT /api/products/{id} — update a product (merchant only).
#[debug_handler]
async fn update_product(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    Json(params): Json<UpdateProductParams>,
) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = product::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.merchant_id != merchant_id {
        return Err(Error::Unauthorized("not the product owner".into()));
    }

    let mut active: product::ActiveModel = existing.into();
    if let Some(name) = params.name {
        active.name = Set(name);
    }
    if let Some(description) = params.description {
        active.description = Set(Some(description));
    }
    if let Some(price_cents) = params.price_cents {
        active.price_cents = Set(price_cents);
    }
    if let Some(discount_price_cents) = params.discount_price_cents {
        active.discount_price_cents = Set(discount_price_cents);
    }
    if let Some(stock) = params.stock {
        active.stock = Set(stock);
    }
    if let Some(category) = params.category {
        active.category = Set(category);
    }
    if let Some(images) = params.images {
        active.images = Set(images);
    }
    if let Some(is_active) = params.is_active {
        active.is_active = Set(if is_active { 1 } else { 0 });
    }
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());

    let updated = product::Entity::update(active).exec(&ctx.db).await?;
    format::json(to_response(&updated))
}

/// DELETE /api/products/{id} — soft delete a product (merchant only).
#[debug_handler]
async fn remove_product(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = product::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.merchant_id != merchant_id {
        return Err(Error::Unauthorized("not the product owner".into()));
    }

    let mut active: product::ActiveModel = existing.into();
    active.is_active = Set(0);
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());
    product::Entity::update(active).exec(&ctx.db).await?;
    format::empty()
}

/// POST /api/products/{id}/upvote — increment upvotes.
#[debug_handler]
async fn upvote_product(State(ctx): State<AppContext>, Path(id): Path<String>) -> Result<Response> {
    let existing = product::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    let mut active: product::ActiveModel = existing.into();
    active.upvotes = Set(active.upvotes.unwrap() + 1);
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());
    let updated = product::Entity::update(active).exec(&ctx.db).await?;
    format::json(to_response(&updated))
}

/// Register product routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/products")
        .add("/", post(create_product))
        .add("/", get(list_products))
        .add("/{id}", get(show_product))
        .add("/{id}", put(update_product))
        .add("/{id}", delete(remove_product))
        .add("/{id}/upvote", post(upvote_product))
}
