//! BOBO checkout bridge.
//!
//! This endpoint is intentionally BOBO-shaped while keeping PR27's app-agnostic
//! commerce primitives underneath it. It creates the deploy-candidate order row
//! for merchant dashboards and a `bobo_orders` row/payment intent for the
//! PR27 escrow/payment lane.

use axum::{
    debug_handler,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use loco_rs::prelude::*;
use sea_orm::{
    prelude::Expr, ActiveValue::Set, ColumnTrait, DatabaseBackend, DatabaseTransaction,
    EntityTrait, FromQueryResult, QueryFilter, Statement, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::{
    bobo_commerce::{self, CommerceError, OrderRow, PaymentIntentRow},
    profile_identity,
};
use yaatal_core::models::{order, order_item, product};

#[derive(Debug, Deserialize)]
pub struct CheckoutItem {
    pub product_id: String,
    pub quantity: i32,
}

#[derive(Debug, Deserialize)]
pub struct CheckoutBody {
    pub buyer_id: String,
    pub seller_id: Option<String>,
    pub product_id: Option<String>,
    pub quantity: Option<i32>,
    pub items: Option<Vec<CheckoutItem>>,
    pub payment_method: String,
    pub delivery_method: Option<String>,
    pub shipping_address: Option<String>,
    pub phone_number: Option<String>,
    pub payer_msisdn: Option<String>,
    pub idempotency_key: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct BoboOrderDto {
    pub id: String,
    pub engine_order_id: String,
    pub bobo_order_id: i64,
    pub buyer_id: String,
    pub seller_id: String,
    pub product_id: String,
    pub quantity: i32,
    pub unit_price: i32,
    pub total_price: i32,
    pub status: String,
    pub payment_method: String,
    pub payment_reference: Option<String>,
    pub shipping_address: Option<String>,
    pub phone_number: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutPaymentDto {
    pub method: String,
    pub status: String,
    pub rail: String,
    pub provider_ref: String,
    pub idempotency_key: Uuid,
    pub amount_xof: i64,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub success: bool,
    pub order: BoboOrderDto,
    pub payment: CheckoutPaymentDto,
}

fn map_error(e: &CommerceError) -> Response {
    use CommerceError::*;
    match e {
        BackendUnsupported => (
            StatusCode::SERVICE_UNAVAILABLE,
            "BOBO checkout requires Postgres",
        )
            .into_response(),
        NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
        BadInput(msg) => (StatusCode::BAD_REQUEST, *msg).into_response(),
        IllegalOrderTransition { .. } | Escrow(_) => {
            (StatusCode::CONFLICT, e.to_string()).into_response()
        }
        Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    }
}

fn normalized_items(body: &CheckoutBody) -> Result<Vec<CheckoutItem>, (StatusCode, &'static str)> {
    if let Some(items) = &body.items {
        if items.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "items required"));
        }
        return Ok(items
            .iter()
            .map(|item| CheckoutItem {
                product_id: item.product_id.clone(),
                quantity: item.quantity,
            })
            .collect());
    }

    let product_id = body
        .product_id
        .clone()
        .ok_or((StatusCode::BAD_REQUEST, "product_id required"))?;
    Ok(vec![CheckoutItem {
        product_id,
        quantity: body.quantity.unwrap_or(1),
    }])
}

fn payment_rail(method: &str) -> Result<(&'static str, &'static str), (StatusCode, &'static str)> {
    match method {
        "cash" => Ok(("Cash", "succeeded")),
        "wave" => Ok(("Wave", "pending")),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "payment_method must be cash or wave for MVP",
        )),
    }
}

fn bobo_status(method: &str, payment_status: &str) -> &'static str {
    match (method, payment_status) {
        ("cash", "succeeded") => "processing",
        (_, "succeeded") => "paid",
        (_, "failed" | "reversed") => "cancelled",
        _ => "pending_payment",
    }
}

fn payment_dto(method: &str, intent: PaymentIntentRow) -> CheckoutPaymentDto {
    CheckoutPaymentDto {
        method: method.to_owned(),
        status: intent.status,
        rail: intent.rail,
        provider_ref: intent.provider_ref,
        idempotency_key: intent.idempotency_key,
        amount_xof: intent.amount_xof,
        redirect_url: None,
    }
}

async fn create_bobo_order_in_txn(
    txn: &DatabaseTransaction,
    merchant_id: String,
    buyer_pid: Uuid,
    total_xof: i64,
) -> Result<OrderRow, CommerceError> {
    if total_xof <= 0 {
        return Err(CommerceError::BadInput("total_xof must be > 0"));
    }

    let sql = r#"
        INSERT INTO bobo_orders (merchant_id, buyer_pid, total_xof, state)
        VALUES ($1, $2, $3, 'created')
        RETURNING id, merchant_id, buyer_pid, total_xof, currency, state, created_at, updated_at
    "#;

    OrderRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [merchant_id.into(), buyer_pid.into(), total_xof.into()],
    ))
    .one(txn)
    .await?
    .ok_or(CommerceError::NotFound)
}

async fn create_payment_intent_in_txn(
    txn: &DatabaseTransaction,
    order_id: i64,
    rail: &str,
    provider_ref: &str,
    idempotency_key: Uuid,
    status: &str,
    amount_xof: i64,
) -> Result<PaymentIntentRow, CommerceError> {
    if amount_xof <= 0 {
        return Err(CommerceError::BadInput("amount_xof must be > 0"));
    }
    if !matches!(status, "pending" | "succeeded" | "failed" | "reversed") {
        return Err(CommerceError::BadInput("invalid payment intent status"));
    }

    let sql = r#"
        INSERT INTO bobo_payment_intents
            (order_id, rail, provider_ref, idempotency_key, status, amount_xof)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (idempotency_key) DO UPDATE
        SET updated_at = now()
        RETURNING order_id, rail, provider_ref, idempotency_key, status, amount_xof, created_at, updated_at
    "#;

    PaymentIntentRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            order_id.into(),
            rail.to_owned().into(),
            provider_ref.to_owned().into(),
            idempotency_key.into(),
            status.to_owned().into(),
            amount_xof.into(),
        ],
    ))
    .one(txn)
    .await?
    .ok_or(CommerceError::NotFound)
}

#[debug_handler]
pub async fn create(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(body): Json<CheckoutBody>,
) -> Response {
    let buyer_pid = match Uuid::parse_str(&auth.claims.pid) {
        Ok(id) => id,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid auth subject").into_response(),
    };
    let buyer_id =
        match profile_identity::ensure_profile_id_for_user_pid(&ctx.db, &auth.claims.pid).await {
            Ok(profile_id) => profile_id,
            Err(_) => return (StatusCode::UNAUTHORIZED, "buyer profile not found").into_response(),
        };
    if !body.buyer_id.is_empty() && body.buyer_id != buyer_id && body.buyer_id != auth.claims.pid {
        return (
            StatusCode::BAD_REQUEST,
            "buyer_id must match authenticated buyer",
        )
            .into_response();
    };
    let items = match normalized_items(&body) {
        Ok(items) => items,
        Err((status, message)) => return (status, message).into_response(),
    };
    if items.iter().any(|item| item.quantity <= 0) {
        return (StatusCode::BAD_REQUEST, "quantity must be > 0").into_response();
    }
    if ctx.db.get_database_backend() != DatabaseBackend::Postgres {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "BOBO checkout requires Postgres",
        )
            .into_response();
    }
    let (rail, payment_status) = match payment_rail(&body.payment_method) {
        Ok(value) => value,
        Err((status, message)) => return (status, message).into_response(),
    };

    let txn = match ctx.db.begin().await {
        Ok(txn) => txn,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    };

    let mut total_cents = 0i32;
    let mut seller_id = body.seller_id.clone();
    let mut first_product_id = String::new();
    let mut first_quantity = 0i32;
    let mut first_unit_price = 0i32;

    for item in &items {
        let product_model = match product::Entity::find_by_id(&item.product_id)
            .one(&txn)
            .await
        {
            Ok(Some(model)) => model,
            Ok(None) => return (StatusCode::NOT_FOUND, "product not found").into_response(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
        };
        if product_model.is_active == 0 {
            return (StatusCode::BAD_REQUEST, "product unavailable").into_response();
        }
        if product_model.stock < item.quantity {
            return (StatusCode::BAD_REQUEST, "insufficient stock").into_response();
        }
        if let Some(existing_seller) = &seller_id {
            if existing_seller != &product_model.merchant_id {
                return (
                    StatusCode::BAD_REQUEST,
                    "all items must use the same seller",
                )
                    .into_response();
            }
        } else {
            seller_id = Some(product_model.merchant_id.clone());
        }

        let unit_price = product_model
            .discount_price_cents
            .unwrap_or(product_model.price_cents);
        total_cents += unit_price * item.quantity;

        if first_product_id.is_empty() {
            first_product_id = item.product_id.clone();
            first_quantity = item.quantity;
            first_unit_price = unit_price;
        }
    }

    let seller_id = match seller_id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, "seller_id required").into_response(),
    };
    let engine_order_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let delivery_method = body
        .delivery_method
        .clone()
        .unwrap_or_else(|| "bobo_managed".to_owned());

    let order_model = order::ActiveModel {
        id: Set(engine_order_id.clone()),
        buyer_id: Set(buyer_id.clone()),
        seller_id: Set(seller_id.clone()),
        status: Set(if body.payment_method == "cash" {
            order::OrderStatus::Confirmed
        } else {
            order::OrderStatus::Pending
        }),
        payment_method: Set(body.payment_method.clone()),
        payment_status: Set(if payment_status == "succeeded" {
            order::PaymentStatus::Paid
        } else {
            order::PaymentStatus::Pending
        }),
        delivery_method: Set(delivery_method),
        total_cents: Set(total_cents),
        created_at: Set(now.clone()),
        updated_at: Set(now.clone()),
    };
    if order::Entity::insert(order_model).exec(&txn).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }

    for item in &items {
        let product_model = match product::Entity::find_by_id(&item.product_id)
            .one(&txn)
            .await
        {
            Ok(Some(model)) => model,
            _ => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
        };
        let unit_price = product_model
            .discount_price_cents
            .unwrap_or(product_model.price_cents);
        let item_model = order_item::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            order_id: Set(engine_order_id.clone()),
            product_id: Set(item.product_id.clone()),
            quantity: Set(item.quantity),
            unit_price_cents: Set(unit_price),
        };
        if order_item::Entity::insert(item_model)
            .exec(&txn)
            .await
            .is_err()
        {
            return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }

        let stock_update = product::Entity::update_many()
            .col_expr(
                product::Column::Stock,
                Expr::col(product::Column::Stock).sub(item.quantity),
            )
            .filter(product::Column::Id.eq(&item.product_id))
            .filter(product::Column::Stock.gte(item.quantity))
            .exec(&txn)
            .await;
        match stock_update {
            Ok(result) if result.rows_affected > 0 => {}
            Ok(_) => return (StatusCode::BAD_REQUEST, "insufficient stock").into_response(),
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
        }
    }

    let bobo_order = match create_bobo_order_in_txn(
        &txn,
        seller_id.clone(),
        buyer_pid,
        total_cents as i64,
    )
    .await
    {
        Ok(order) => order,
        Err(ref e) => return map_error(e),
    };

    let idempotency_key = body.idempotency_key.unwrap_or_else(Uuid::new_v4);
    let provider_ref = format!(
        "{}_stub_{}",
        body.payment_method.replace('_', "-"),
        bobo_order.id
    );
    let intent = match create_payment_intent_in_txn(
        &txn,
        bobo_order.id,
        rail,
        &provider_ref,
        idempotency_key,
        payment_status,
        total_cents as i64,
    )
    .await
    {
        Ok(intent) => intent,
        Err(ref e) => return map_error(e),
    };

    if txn.commit().await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }

    let order = BoboOrderDto {
        id: engine_order_id.clone(),
        engine_order_id,
        bobo_order_id: bobo_order.id,
        buyer_id,
        seller_id,
        product_id: first_product_id,
        quantity: first_quantity,
        unit_price: first_unit_price,
        total_price: total_cents,
        status: bobo_status(&body.payment_method, payment_status).to_owned(),
        payment_method: body.payment_method.clone(),
        payment_reference: Some(provider_ref),
        shipping_address: body.shipping_address,
        phone_number: body.phone_number.or(body.payer_msisdn),
        created: now.clone(),
        updated: now,
    };

    (
        StatusCode::CREATED,
        Json(CheckoutResponse {
            success: true,
            order,
            payment: payment_dto(&body.payment_method, intent),
        }),
    )
        .into_response()
}

#[debug_handler]
pub async fn payment_status(State(ctx): State<AppContext>, Path(order_id): Path<i64>) -> Response {
    match bobo_commerce::get_payment_intent_for_order(&ctx.db, order_id).await {
        Ok(intent) => Json(payment_dto("wave", intent)).into_response(),
        Err(ref e) => map_error(e),
    }
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/bobo")
        .add("/checkout", post(create))
        .add("/checkout/{order_id}/payment", get(payment_status))
}
