//! Merchant dashboard controller — JWT-authenticated merchant endpoints.

use loco_rs::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::Deserialize;

use crate::{
    services::profile_identity,
    views::orders::{DashboardResponse, OrderListResponse, OrderResponse},
    views::products::{ProductListResponse, ProductResponse},
};
use yaatal_core::models::{order, product};

/// Query params for listing merchant data.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

fn product_to_response(m: &product::Model) -> ProductResponse {
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

async fn build_order_response(
    db: &sea_orm::DatabaseConnection,
    order_model: &order::Model,
) -> Result<OrderResponse> {
    use yaatal_core::models::order_item;

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
            .map(|i| crate::views::orders::OrderItemResponse {
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

/// GET /api/merchant/dashboard — merchant stats.
#[debug_handler]
async fn dashboard(auth: auth::JWT, State(ctx): State<AppContext>) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let orders = order::Entity::find()
        .filter(order::Column::SellerId.eq(&merchant_id))
        .all(&ctx.db)
        .await?;

    let mut total_revenue = 0i64;
    let mut total_orders = 0i64;
    let mut orders_by_status = std::collections::HashMap::<String, i64>::new();

    for o in orders {
        total_orders += 1;
        let status_str = format!("{:?}", o.status).to_lowercase();
        *orders_by_status.entry(status_str.clone()).or_insert(0) += 1;

        if o.status == order::OrderStatus::Delivered {
            total_revenue += o.total_cents as i64;
        }
    }

    format::json(DashboardResponse {
        total_revenue_cents: total_revenue,
        total_orders,
        orders_by_status,
    })
}

/// GET /api/merchant/products — list merchant's products.
#[debug_handler]
async fn merchant_products(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let page = params.page.unwrap_or(1).max(1) - 1;
    let per_page = params.per_page.unwrap_or(20).min(100);

    let paginator = product::Entity::find()
        .filter(product::Column::MerchantId.eq(&merchant_id))
        .order_by_desc(product::Column::CreatedAt)
        .paginate(&ctx.db, per_page);

    let total = paginator.num_items().await?;
    let products: Vec<product::Model> = paginator.fetch_page(page).await?;

    format::json(ProductListResponse {
        products: products.iter().map(product_to_response).collect(),
        total,
        page: page + 1,
        per_page,
    })
}

/// GET /api/merchant/orders — list incoming orders for merchant.
#[debug_handler]
async fn merchant_orders(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let merchant_id =
        profile_identity::resolve_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await?;

    let page = params.page.unwrap_or(1).max(1) - 1;
    let per_page = params.per_page.unwrap_or(20).min(100);

    let paginator = order::Entity::find()
        .filter(order::Column::SellerId.eq(&merchant_id))
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

/// Register merchant routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/merchant")
        .add("/dashboard", get(dashboard))
        .add("/products", get(merchant_products))
        .add("/orders", get(merchant_orders))
}
