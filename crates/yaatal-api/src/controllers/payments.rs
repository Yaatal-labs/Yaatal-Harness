//! Payments HTTP surface — three endpoints wired to `PaymentsService`.
//!
//! - `POST /api/payments/initiate`        — JWT-protected; initiates a payment.
//! - `POST /api/payments/wave/webhook`    — unauthenticated; HMAC-gated webhook.
//! - `GET  /api/payments/{provider_ref}/poll` — JWT-protected; polls status.
//!
//! Error mapping keeps `InvalidCallback` opaque toward the caller to prevent
//! HMAC oracle attacks.

use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use loco_rs::prelude::*;
use serde::Serialize;
use tracing::warn;
use yaatal_payments::{
    contract::{PaymentRequest, Rail},
    error::PaymentError,
    webhook::RawCallback,
};

use crate::services::payments_service::PaymentsService;

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_error(e: &PaymentError) -> Response {
    match e {
        PaymentError::RailNotConfigured(_) => {
            (StatusCode::BAD_REQUEST, "rail not configured").into_response()
        }
        PaymentError::IdempotencyConflict => {
            (StatusCode::CONFLICT, "idempotency conflict").into_response()
        }
        PaymentError::InvalidCallback(msg) => {
            // Do NOT echo the message — it could reveal HMAC oracle info.
            warn!(detail = msg, "invalid callback received");
            (StatusCode::BAD_REQUEST, "invalid signature").into_response()
        }
        PaymentError::ProviderRejected(_) => {
            (StatusCode::BAD_GATEWAY, "provider rejected").into_response()
        }
        PaymentError::Transport(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "provider unreachable").into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/payments/initiate
// ---------------------------------------------------------------------------

/// JWT-protected. Accepts `PaymentRequest` JSON, returns `PaymentHandle` JSON.
#[debug_handler]
pub async fn initiate(
    _auth: auth::JWT,
    axum::Extension(svc): axum::Extension<Arc<PaymentsService>>,
    State(_ctx): State<AppContext>,
    Json(req): Json<PaymentRequest>,
) -> Response {
    match svc.initiate(&req).await {
        Ok(handle) => Json(handle).into_response(),
        Err(ref e) => map_error(e),
    }
}

// ---------------------------------------------------------------------------
// POST /api/payments/wave/webhook
// ---------------------------------------------------------------------------

/// Unauthenticated. HMAC-SHA256 over the raw body is the gate.
/// Headers are forwarded so the `WaveAdapter::verify_signature` can find
/// `X-Wave-Signature`.
#[debug_handler]
pub async fn wave_webhook(
    axum::Extension(svc): axum::Extension<Arc<PaymentsService>>,
    State(_ctx): State<AppContext>,
    req: Request,
) -> Response {
    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_owned(), v.to_owned()))
        })
        .collect();

    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b.to_vec(),
        Err(e) => {
            warn!(error = %e, "failed to read webhook body");
            return (StatusCode::BAD_REQUEST, "invalid signature").into_response();
        }
    };

    let raw = RawCallback {
        rail: Rail::Wave,
        headers,
        body,
    };

    match svc.dispatch_webhook(raw).await {
        Ok(result) => Json(result).into_response(),
        Err(ref e) => map_error(e),
    }
}

// ---------------------------------------------------------------------------
// GET /api/payments/{provider_ref}/poll
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PollResponse {
    status: String,
}

/// JWT-protected. Returns `{ "status": "..." }`.
#[debug_handler]
pub async fn poll(
    _auth: auth::JWT,
    axum::Extension(svc): axum::Extension<Arc<PaymentsService>>,
    State(_ctx): State<AppContext>,
    Path(provider_ref): Path<String>,
) -> Response {
    match svc.poll(Rail::Wave, &provider_ref).await {
        Ok(status) => {
            let body = PollResponse {
                status: format!("{status:?}").to_lowercase(),
            };
            Json(body).into_response()
        }
        Err(ref e) => map_error(e),
    }
}

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/payments")
        .add("/initiate", post(initiate))
        .add("/wave/webhook", post(wave_webhook))
        .add("/{provider_ref}/poll", get(poll))
}
