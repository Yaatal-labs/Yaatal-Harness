//! BOBO KYC HTTP surface — submit + check status for the authenticated user.
//!
//! Routes (both JWT-protected):
//! - `POST /api/bobo/kyc`  — submit a hashed document for verification
//! - `GET  /api/bobo/kyc`  — current user's KYC status
//!
//! Raw documents are never accepted by this surface — only the SHA-256 digest.

use axum::{
    debug_handler,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use loco_rs::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::services::bobo_commerce::{self, CommerceError};

fn map_error(e: &CommerceError) -> Response {
    use CommerceError::*;
    match e {
        BackendUnsupported => (
            StatusCode::SERVICE_UNAVAILABLE,
            "BOBO KYC requires Postgres",
        )
            .into_response(),
        NotFound => (StatusCode::NOT_FOUND, "no KYC profile").into_response(),
        BadInput(msg) => (StatusCode::BAD_REQUEST, *msg).into_response(),
        Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
        IllegalOrderTransition { .. } | Escrow(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "unexpected").into_response()
        }
    }
}

fn parse_pid(claims_pid: &str) -> Result<Uuid, Box<Response>> {
    Uuid::parse_str(claims_pid)
        .map_err(|_| Box::new((StatusCode::UNAUTHORIZED, "invalid pid in JWT").into_response()))
}

#[derive(Debug, Deserialize)]
pub struct SubmitKycBody {
    /// "smile_id" | "manual" — provider name.
    pub provider: String,
    /// Base64-encoded SHA-256 digest of the identity document (exactly 32 bytes
    /// decoded). The raw document is never transmitted or persisted.
    pub document_hash_b64: String,
    /// ISO-3166-1 alpha-2 country code (e.g. "SN", "CI", "FR").
    pub jurisdiction: String,
}

#[debug_handler]
pub async fn submit(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(body): Json<SubmitKycBody>,
) -> Response {
    let pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let hash = match B64.decode(body.document_hash_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "document_hash_b64 must be valid base64",
            )
                .into_response()
        }
    };
    match bobo_commerce::submit_kyc(&ctx.db, pid, &body.provider, hash, &body.jurisdiction).await {
        Ok(row) => (StatusCode::ACCEPTED, Json(row)).into_response(),
        Err(ref e) => map_error(e),
    }
}

#[debug_handler]
pub async fn status(auth: auth::JWT, State(ctx): State<AppContext>) -> Response {
    let pid = match parse_pid(&auth.claims.pid) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    match bobo_commerce::get_kyc(&ctx.db, pid).await {
        Ok(row) => Json(row).into_response(),
        Err(ref e) => map_error(e),
    }
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/bobo/kyc")
        .add("/", post(submit))
        .add("/", get(status))
}
