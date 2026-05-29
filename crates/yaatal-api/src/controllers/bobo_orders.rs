//! BOBO orders HTTP surface — buyer-side order lifecycle + escrow read view.
//!
//! All routes JWT-protected and scoped to the authenticated buyer (`auth.claims.pid`).
//! Merchant-side operations (settle, dispute resolution) ship in a later lane.
//!
//! Routes:
//! - `POST /api/bobo/orders`                              — create
//! - `GET  /api/bobo/orders`                              — list (buyer scope)
//! - `GET  /api/bobo/orders/{id}`                         — detail
//! - `GET  /api/bobo/orders/{id}/escrow`                  — escrow state read
//! - `POST /api/bobo/orders/{id}/simulate-payment`        — DOGFOOD ONLY
//! - `POST /api/bobo/orders/{id}/confirm-delivery`        — buyer marks received
//! - `POST /api/bobo/orders/{id}/dispute`                 — buyer raises dispute
//! - `POST /api/bobo/orders/{id}/cancel`                  — buyer cancels (created-only)

use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use yaatal_analytics::{AnalyticsDispatcher, AnalyticsEvent};

use crate::services::bobo_commerce::{self, CommerceError, EscrowRow, OrderRow};

// ─── Error mapping ─────────────────────────────────────────────────────────────

fn map_error(e: &CommerceError) -> Response {
    use CommerceError::*;
    match e {
        BackendUnsupported => (
            StatusCode::SERVICE_UNAVAILABLE,
            "BOBO commerce requires Postgres",
        )
            .into_response(),
        NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
        IllegalOrderTransition { .. } => (StatusCode::CONFLICT, e.to_string()).into_response(),
        Escrow(_) => (StatusCode::CONFLICT, e.to_string()).into_response(),
        BadInput(msg) => (StatusCode::BAD_REQUEST, *msg).into_response(),
        Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    }
}

fn parse_pid(claims_pid: &str) -> Result<Uuid, Box<Response>> {
    Uuid::parse_str(claims_pid)
        .map_err(|_| Box::new((StatusCode::UNAUTHORIZED, "invalid pid in JWT").into_response()))
}

// ─── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOrderBody {
    pub merchant_id: String,
    pub total_xof: i64,
    pub delivery_lat: Option<f64>,
    pub delivery_lng: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListOrdersQuery {
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OrderWithEscrow {
    #[serde(flatten)]
    pub order: OrderRow,
    pub escrow: Option<EscrowRow>,
}

// ─── Handlers ──────────────────────────────────────────────────────────────────

#[debug_handler]
pub async fn create(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(body): Json<CreateOrderBody>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::create_order(
        &ctx.db,
        body.merchant_id,
        buyer_pid,
        body.total_xof,
        body.delivery_lat,
        body.delivery_lng,
    )
    .await
    {
        Ok(o) => (StatusCode::CREATED, Json(o)).into_response(),
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn list(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(q): Query<ListOrdersQuery>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let limit = q.limit.unwrap_or(50).min(200);
    match bobo_commerce::list_orders_for_buyer(&ctx.db, buyer_pid, limit).await {
        Ok(rows) => Json(rows).into_response(),
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn show(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(order_id): Path<i64>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let order = match bobo_commerce::get_order(&ctx.db, order_id, buyer_pid).await {
        Ok(o) => o,
        Err(ref e) => return map_error(e),
    };
    let escrow = bobo_commerce::get_escrow(&ctx.db, order_id)
        .await
        .ok()
        .flatten();
    Json(OrderWithEscrow { order, escrow }).into_response()
}

#[debug_handler]
pub async fn show_escrow(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(order_id): Path<i64>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    // Ownership check via get_order so we don't leak escrows of other buyers.
    if let Err(ref e) = bobo_commerce::get_order(&ctx.db, order_id, buyer_pid).await {
        return map_error(e);
    }
    match bobo_commerce::get_escrow(&ctx.db, order_id).await {
        Ok(Some(row)) => Json(row).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no escrow yet").into_response(),
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn simulate_payment(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(order_id): Path<i64>,
) -> Response {
    // DOGFOOD ONLY — production path is the payment webhook bridge (TODO Lane 5c).
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::simulate_payment(&ctx.db, order_id, buyer_pid).await {
        Ok((order, escrow)) => {
            tracing::info!(order_id, %buyer_pid, "DOGFOOD simulate-payment");
            Json(OrderWithEscrow {
                order,
                escrow: Some(escrow),
            })
            .into_response()
        }
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn confirm_delivery(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Extension(analytics): Extension<Arc<AnalyticsDispatcher>>,
    Path(order_id): Path<i64>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::confirm_delivery(&ctx.db, order_id, buyer_pid).await {
        Ok((order, escrow)) => {
            analytics.capture(AnalyticsEvent {
                name: "bobo.escrow.transitioned",
                distinct_id: buyer_pid.to_string(),
                properties: json!({
                    "order_id": order_id,
                    "from": "held",
                    "to": escrow.state,
                    "trigger": "confirm_delivery",
                }),
            });
            Json(OrderWithEscrow {
                order,
                escrow: Some(escrow),
            })
            .into_response()
        }
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn dispute(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Extension(analytics): Extension<Arc<AnalyticsDispatcher>>,
    Path(order_id): Path<i64>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::dispute_order(&ctx.db, order_id, buyer_pid).await {
        Ok((order, escrow)) => {
            analytics.capture(AnalyticsEvent {
                name: "bobo.escrow.transitioned",
                distinct_id: buyer_pid.to_string(),
                properties: json!({
                    "order_id": order_id,
                    "from": "held",
                    "to": escrow.state,
                    "trigger": "dispute",
                }),
            });
            Json(OrderWithEscrow {
                order,
                escrow: Some(escrow),
            })
            .into_response()
        }
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn cancel(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Path(order_id): Path<i64>,
) -> Response {
    let buyer_pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::cancel_order(&ctx.db, order_id, buyer_pid).await {
        Ok(o) => Json(o).into_response(),
        Err(ref e) => map_error(e),
    }
}

// ─── Route registration ───────────────────────────────────────────────────────

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/bobo/orders")
        .add("/", post(create))
        .add("/", get(list))
        .add("/{order_id}", get(show))
        .add("/{order_id}/escrow", get(show_escrow))
        .add("/{order_id}/simulate-payment", post(simulate_payment))
        .add("/{order_id}/confirm-delivery", post(confirm_delivery))
        .add("/{order_id}/dispute", post(dispute))
        .add("/{order_id}/cancel", post(cancel))
}
