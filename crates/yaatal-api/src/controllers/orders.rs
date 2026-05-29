//! Orders controller — JWT-authenticated order management.

use axum::extract::Path;
use loco_rs::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    services::profile_identity,
    views::orders::{OrderItemResponse, OrderListResponse, OrderResponse},
};
use yaatal_core::models::{order, order_item, product};

/// Request body for creating an order item.
#[derive(Debug, Deserialize)]
pub struct OrderItemParams {
    pub product_id: String,
    pub quantity: i32,
}

/// Request body for creating an order.
#[derive(Debug, Deserialize)]
pub struct CreateOrderParams {
    pub seller_id: String,
    pub items: Vec<OrderItemParams>,
    pub payment_method: String,
    pub delivery_method: String,
}

/// Request body for updating order status.
#[derive(Debug, Deserialize)]
pub struct UpdateStatusParams {
    pub status: String,
}

/// Request body for updating payment status.
#[derive(Debug, Deserialize)]
pub struct UpdatePaymentParams {
    pub payment_status: String,
}

/// Query params for listing orders.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

fn order_status_from_str(s: &str) -> Result<order::OrderStatus> {
    match s {
        "pending" => Ok(order::OrderStatus::Pending),
        "confirmed" => Ok(order::OrderStatus::Confirmed),
        "shipped" => Ok(order::OrderStatus::Shipped),
        "delivered" => Ok(order::OrderStatus::Delivered),
        "cancelled" => Ok(order::OrderStatus::Cancelled),
        _ => Err(Error::BadRequest("invalid order status".into())),
    }
}

/// Validate status transitions according to business rules.
fn is_valid_status_transition(current: &order::OrderStatus, next: &order::OrderStatus) -> bool {
    match (current, next) {
        // Pending can go to Confirmed or Cancelled
        (order::OrderStatus::Pending, order::OrderStatus::Confirmed) => true,
        (order::OrderStatus::Pending, order::OrderStatus::Cancelled) => true,
        // Confirmed can go to Shipped
        (order::OrderStatus::Confirmed, order::OrderStatus::Shipped) => true,
        // Shipped can go to Delivered
        (order::OrderStatus::Shipped, order::OrderStatus::Delivered) => true,
        // No other transitions allowed
        _ => false,
    }
}

fn payment_status_from_str(s: &str) -> order::PaymentStatus {
    match s {
        "paid" => order::PaymentStatus::Paid,
        "failed" => order::PaymentStatus::Failed,
        _ => order::PaymentStatus::Pending,
    }
}

async fn build_order_response(
    db: &sea_orm::DatabaseConnection,
    order_model: &order::Model,
) -> Result<OrderResponse> {
    let items = order_item::Entity::find()
        .filter(order_item::Column::OrderId.eq(&order_model.id))
        .all(db)
        .await?;

    Ok(OrderResponse {
        id: order_model.id.clone(),
        buyer_id: order_model.buyer_id.clone(),
        seller_id: order_model.seller_id.clone(),
        status: format!("{:?}", order_model.status).to_lowercase(),
        payment_method: order_model.payment_method.clone(),
        payment_status: format!("{:?}", order_model.payment_status).to_lowercase(),
        delivery_method: order_model.delivery_method.clone(),
        total_cents: order_model.total_cents,
        items: items
            .iter()
            .map(|i| OrderItemResponse {
                id: i.id.clone(),
                product_id: i.product_id.clone(),
                quantity: i.quantity,
                unit_price_cents: i.unit_price_cents,
            })
            .collect(),
        created_at: order_model.created_at.clone(),
        updated_at: Some(order_model.updated_at.clone()),
    })
}

/// POST /api/orders — create a new order.
#[debug_handler]
async fn create_order(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(params): Json<CreateOrderParams>,
) -> Result<Response> {
    let buyer_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    // Validate seller exists
    yaatal_core::models::profile::Entity::find_by_id(&params.seller_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::BadRequest("invalid seller".into()))?;

    let txn = ctx.db.begin().await?;

    let order_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mut total_cents = 0i32;

    // Validate items and calculate total
    for item in &params.items {
        let product_model = product::Entity::find_by_id(&item.product_id)
            .one(&txn)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        if product_model.is_active == 0 {
            return Err(Error::BadRequest("product unavailable".into()));
        }

        if product_model.stock < item.quantity {
            return Err(Error::BadRequest("insufficient stock".into()));
        }

        let unit_price = product_model
            .discount_price_cents
            .unwrap_or(product_model.price_cents);
        total_cents += unit_price * item.quantity;
    }

    // Create order
    let order_model = order::ActiveModel {
        id: Set(order_id.clone()),
        buyer_id: Set(buyer_id),
        seller_id: Set(params.seller_id.clone()),
        status: Set(order::OrderStatus::Pending),
        payment_method: Set(params.payment_method.clone()),
        payment_status: Set(order::PaymentStatus::Pending),
        delivery_method: Set(params.delivery_method.clone()),
        total_cents: Set(total_cents),
        created_at: Set(now.clone()),
        updated_at: Set(now.clone()),
    };
    order::Entity::insert(order_model).exec(&txn).await?;

    // Create order items and atomically decrement stock
    for item in &params.items {
        let product_model = product::Entity::find_by_id(&item.product_id)
            .one(&txn)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let unit_price = product_model
            .discount_price_cents
            .unwrap_or(product_model.price_cents);
        let item_id = Uuid::new_v4().to_string();
        let item_model = order_item::ActiveModel {
            id: Set(item_id),
            order_id: Set(order_id.clone()),
            product_id: Set(item.product_id.clone()),
            quantity: Set(item.quantity),
            unit_price_cents: Set(unit_price),
        };
        order_item::Entity::insert(item_model).exec(&txn).await?;

        // Atomic stock decrement — prevents race conditions
        let update_result = product::Entity::update_many()
            .col_expr(
                product::Column::Stock,
                Expr::col(product::Column::Stock).sub(item.quantity),
            )
            .filter(product::Column::Id.eq(&item.product_id))
            .filter(product::Column::Stock.gte(item.quantity))
            .exec(&txn)
            .await?;

        if update_result.rows_affected == 0 {
            return Err(Error::BadRequest("insufficient stock".into()));
        }
    }

    txn.commit().await?;

    let created = order::Entity::find_by_id(&order_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    format::json(build_order_response(&ctx.db, &created).await?)
}

/// GET /api/orders/me — list buyer's orders.
#[debug_handler]
async fn my_orders(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let buyer_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let page = params.page.unwrap_or(1).max(1) - 1;
    let per_page = params.per_page.unwrap_or(20).min(100);

    let paginator = order::Entity::find()
        .filter(order::Column::BuyerId.eq(buyer_id))
        .order_by_desc(order::Column::CreatedAt)
        .paginate(&ctx.db, per_page);

    let total = paginator.num_items().await?;
    let orders = paginator.fetch_page(page).await?;

    let mut responses = Vec::new();
    for o in orders {
        responses.push(build_order_response(&ctx.db, &o).await?);
    }

    format::json(OrderListResponse {
        orders: responses,
        total,
        page: page + 1,
        per_page,
    })
}

/// GET /api/orders/{id} — get order detail.
#[debug_handler]
async fn show_order(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let order_model = order::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Only buyer or seller can view
    if order_model.buyer_id != profile_id && order_model.seller_id != profile_id {
        return Err(Error::Unauthorized(
            "not authorized to view this order".into(),
        ));
    }

    format::json(build_order_response(&ctx.db, &order_model).await?)
}

/// PATCH /api/orders/{id}/status — update order status (seller only).
#[debug_handler]
async fn update_status(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    Json(params): Json<UpdateStatusParams>,
) -> Result<Response> {
    let seller_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = order::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.seller_id != seller_id {
        return Err(Error::Unauthorized("not the order seller".into()));
    }

    let new_status = order_status_from_str(&params.status)?;
    if !is_valid_status_transition(&existing.status, &new_status) {
        return Err(Error::BadRequest("invalid status transition".into()));
    }

    let mut active: order::ActiveModel = existing.into();
    active.status = Set(new_status);
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());
    let updated = order::Entity::update(active).exec(&ctx.db).await?;

    format::json(build_order_response(&ctx.db, &updated).await?)
}

/// PATCH /api/orders/{id}/payment — update payment status.
#[debug_handler]
async fn update_payment(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
    Json(params): Json<UpdatePaymentParams>,
) -> Result<Response> {
    let profile_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = order::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Either buyer or seller can update payment status
    if existing.buyer_id != profile_id && existing.seller_id != profile_id {
        return Err(Error::Unauthorized("not authorized".into()));
    }

    let mut active: order::ActiveModel = existing.into();
    active.payment_status = Set(payment_status_from_str(&params.payment_status));
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());
    let updated = order::Entity::update(active).exec(&ctx.db).await?;

    format::json(build_order_response(&ctx.db, &updated).await?)
}

/// POST /api/orders/{id}/cancel — cancel an order (buyer only, if pending).
#[debug_handler]
async fn cancel_order(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Response> {
    let buyer_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;
    let existing = order::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    if existing.buyer_id != buyer_id {
        return Err(Error::Unauthorized("not the order buyer".into()));
    }

    if existing.status != order::OrderStatus::Pending {
        return Err(Error::BadRequest("can only cancel pending orders".into()));
    }

    let txn = ctx.db.begin().await?;

    // Restore stock
    let items = order_item::Entity::find()
        .filter(order_item::Column::OrderId.eq(&id))
        .all(&txn)
        .await?;

    for item in items {
        let product_model = product::Entity::find_by_id(&item.product_id)
            .one(&txn)
            .await?
            .ok_or_else(|| Error::NotFound)?;
        let mut product_active: product::ActiveModel = product_model.into();
        product_active.stock = Set(product_active.stock.unwrap() + item.quantity);
        product_active.updated_at = Set(chrono::Utc::now().to_rfc3339());
        product::Entity::update(product_active).exec(&txn).await?;
    }

    // Cancel order
    let mut active: order::ActiveModel = existing.into();
    active.status = Set(order::OrderStatus::Cancelled);
    active.updated_at = Set(chrono::Utc::now().to_rfc3339());
    order::Entity::update(active).exec(&txn).await?;

    txn.commit().await?;

    let updated = order::Entity::find_by_id(&id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    format::json(build_order_response(&ctx.db, &updated).await?)
}

/// Register order routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/orders")
        .add("/", post(create_order))
        .add("/", get(my_orders))
        .add("/me", get(my_orders))
        .add("/{id}", get(show_order))
        .add("/{id}/status", patch(update_status))
        .add("/{id}/payment", patch(update_payment))
        .add("/{id}/cancel", post(cancel_order))
}
