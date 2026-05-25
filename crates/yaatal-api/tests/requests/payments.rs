//! Integration tests for `/api/payments/*` endpoints.
//!
//! These tests set WAVE_* env vars so `PaymentsService::from_env` succeeds and
//! the `Extension` layer is attached during `App::after_routes`. Each test
//! must be run serially (via `#[serial]`) to avoid env-var races.
//!
//! Test 1: webhook with wrong signature → 400
//! Test 2: webhook with valid HMAC-SHA256 hex signature → 200 + PaymentResult JSON
//! Test 3: POST /initiate with JWT + valid PaymentRequest → 200 + PaymentHandle JSON

#![allow(clippy::unwrap_used, clippy::expect_used)]

use hmac::{Hmac, Mac};
use loco_rs::testing::prelude::*;
use serial_test::serial;
use sha2::Sha256;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
use yaatal_api::app::App;

use super::prepare_data;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const WAVE_SECRET_HEX: &str = "deadbeefdeadbeefdeadbeefdeadbeef"; // 16-byte key

fn hmac_sign(secret_hex: &str, body: &[u8]) -> String {
    let secret = hex::decode(secret_hex).expect("decode hex secret");
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&secret).expect("hmac key");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

fn set_wave_env(api_base: &str) {
    // Safety: test-only; serial tests avoid races.
    unsafe {
        std::env::set_var("WAVE_API_BASE", api_base);
        std::env::set_var("WAVE_API_KEY", "test_api_key");
        std::env::set_var("WAVE_WEBHOOK_SECRET", WAVE_SECRET_HEX);
        std::env::set_var("WAVE_MERCHANT_ID", "M_TEST");
    }
}

fn clear_wave_env() {
    unsafe {
        std::env::remove_var("WAVE_API_BASE");
        std::env::remove_var("WAVE_API_KEY");
        std::env::remove_var("WAVE_WEBHOOK_SECRET");
        std::env::remove_var("WAVE_MERCHANT_ID");
    }
}

// ---------------------------------------------------------------------------
// Test 1: unsigned (wrong) webhook → 400
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn webhook_with_wrong_signature_returns_400() {
    // Use a throwaway server URI; no real calls happen because sig check fails first.
    set_wave_env("http://127.0.0.1:19999");

    request::<App, _, _>(|request, _ctx| async move {
        let body = b"{\"id\":\"WV_001\",\"status\":\"succeeded\",\"amount\":5000}";

        // Deliberately incorrect signature
        let response = request
            .post("/api/payments/wave/webhook")
            .add_header(
                axum::http::HeaderName::from_static("x-wave-signature"),
                axum::http::HeaderValue::from_static("00000000000000000000000000000000"),
            )
            .bytes(body.as_ref().into())
            .await;

        assert_eq!(
            response.status_code(),
            400,
            "wrong signature must yield 400, got: {} — {}",
            response.status_code(),
            response.text()
        );
        assert_eq!(response.text(), "invalid signature");
    })
    .await;

    clear_wave_env();
}

// ---------------------------------------------------------------------------
// Test 2: valid HMAC webhook → 200 + result JSON
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn webhook_with_valid_signature_returns_200() {
    let mock_server = MockServer::start().await;
    set_wave_env(&mock_server.uri());

    // Wave doesn't need to be called for a confirm-only webhook, but we set it
    // up anyway so the adapter's HTTP client has a valid base URL.
    let body = serde_json::to_vec(&serde_json::json!({
        "id": "WV_SESSION_101",
        "status": "succeeded",
        "client_reference": "order_99",
        "amount": 10_000,
        "fees": 100
    }))
    .expect("body");
    let sig = hmac_sign(WAVE_SECRET_HEX, &body);

    request::<App, _, _>(move |request, _ctx| {
        let body = body.clone();
        let sig = sig.clone();
        async move {
            let response = request
                .post("/api/payments/wave/webhook")
                .add_header(
                    axum::http::HeaderName::from_static("x-wave-signature"),
                    axum::http::HeaderValue::from_str(&sig).expect("sig header"),
                )
                .bytes(axum::body::Bytes::from(body))
                .await;

            assert_eq!(
                response.status_code(),
                200,
                "valid webhook must yield 200, got: {} — {}",
                response.status_code(),
                response.text()
            );
            let parsed: serde_json::Value =
                serde_json::from_str(&response.text()).expect("parse JSON");
            assert_eq!(parsed["status"], "Succeeded");
            assert_eq!(parsed["provider_ref"], "WV_SESSION_101");
            assert_eq!(parsed["amount"], 10_000);
        }
    })
    .await;

    clear_wave_env();
}

// ---------------------------------------------------------------------------
// Test 3: initiate with JWT + valid PaymentRequest → 200 + PaymentHandle JSON
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn initiate_with_jwt_returns_200_with_handle() {
    let mock_server = MockServer::start().await;
    set_wave_env(&mock_server.uri());

    // Mock the Wave checkout endpoint.
    Mock::given(method("POST"))
        .and(path("/checkout/sessions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "WV_SESSION_202"
            })),
        )
        .mount(&mock_server)
        .await;

    request::<App, _, _>(|request, ctx| async move {
        let logged_in = prepare_data::init_user_login(&request, &ctx).await;

        let payload = serde_json::json!({
            "amount": 5000,
            "currency": "Xof",
            "reference": "order_42",
            "idempotency_key": "550e8400-e29b-41d4-a716-446655440000",
            "instrument_hint": "Wave",
            "payer_msisdn": "221770000000"
        });

        let (header_name, header_value) = prepare_data::auth_header(&logged_in.token);

        let response = request
            .post("/api/payments/initiate")
            .add_header(header_name, header_value)
            .json(&payload)
            .await;

        assert_eq!(
            response.status_code(),
            200,
            "initiate must yield 200, got: {} — {}",
            response.status_code(),
            response.text()
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&response.text()).expect("parse JSON");
        assert_eq!(parsed["rail"], "Wave");
        assert_eq!(parsed["provider_ref"], "WV_SESSION_202");
    })
    .await;

    clear_wave_env();
}
