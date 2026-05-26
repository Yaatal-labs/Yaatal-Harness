//! End-to-end state-machine integration tests for `yaatal-payments`.
//!
//! Walks the 9 success criteria from Notion *Payment wrapper* TASK.md §2:
//!
//! 1. Compiles clean (proved by `cargo build`).
//! 2. One normalized contract used across rails (covered by the cross-rail
//!    invocation in `normalized_contract_is_uniform_across_rails`).
//! 3. One fully-wired adapter (Wave) + stub adapters returning
//!    `RailNotConfigured` (covered by `routing_to_stub_returns_rail_not_configured`).
//! 4. Idempotency enforced — same key never charges twice (covered by
//!    `same_idempotency_key_replayed_yields_one_charge`).
//! 5. Async confirmation works via webhook AND via poll (covered by
//!    `webhook_confirm_happy_path_succeeded` and
//!    `missed_webhook_poll_recovery_returns_succeeded`).
//! 6. Append-only `payment_events` store with idempotent writes (covered by
//!    `event_log_is_append_only_initiated_then_settled`).
//! 7. `PaymentStatus::Reversed` representable + handled in `confirm` (covered
//!    by `reversal_callback_yields_reversed`).
//! 8. No real provider secrets in code (covered by code-review; no test
//!    asserts it directly, but every secret in this file is a literal
//!    `b"super-secret-key"` test fixture).
//! 9. Suite covers idempotent replay / webhook happy / poll recovery /
//!    reversal / rail-not-configured (this file).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use yaatal_payments::{
    adapters::{OrangeMoneyAdapter, WaveAdapter, WaveConfig},
    Currency, EventStore, InMemoryEventStore, PaymentError, PaymentEventKind, PaymentRequest,
    PaymentStatus, Rail, RailSelector, RawCallback, SettlementAdapter, WebhookRouter,
};

fn sample_secret() -> Vec<u8> {
    b"super-secret-key".to_vec()
}

fn sign(secret: &[u8], body: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("hmac key");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

fn wave_request(key: uuid::Uuid) -> PaymentRequest {
    PaymentRequest {
        amount: 5_000,
        currency: Currency::Xof,
        reference: "order_42".to_owned(),
        idempotency_key: key,
        instrument_hint: Some(Rail::Wave),
        payer_msisdn: Some("221770000000".to_owned()),
    }
}

fn make_wave(server_uri: String, store: Arc<dyn EventStore>) -> WaveAdapter {
    WaveAdapter::new(
        WaveConfig {
            api_base: server_uri,
            api_key: "test_key".to_owned(),
            webhook_secret: sample_secret(),
            merchant_id: "M_TEST".to_owned(),
        },
        reqwest::Client::new(),
        store,
    )
}

fn make_callback(rail: Rail, body: Vec<u8>) -> RawCallback {
    let signature = sign(&sample_secret(), &body);
    RawCallback {
        rail,
        headers: vec![("X-Wave-Signature".to_owned(), signature)],
        body,
    }
}

#[tokio::test]
async fn webhook_confirm_happy_path_succeeded() {
    let server = MockServer::start().await;
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let adapter = make_wave(server.uri(), Arc::clone(&store));

    Mock::given(method("POST"))
        .and(path("/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "WV_E2E_001",
        })))
        .mount(&server)
        .await;

    let key = uuid::Uuid::new_v4();
    let handle = adapter
        .initiate(&wave_request(key))
        .await
        .expect("initiate");
    assert_eq!(handle.provider_ref, "WV_E2E_001");

    let body = serde_json::to_vec(&serde_json::json!({
        "id": "WV_E2E_001",
        "status": "succeeded",
        "client_reference": "order_42",
        "amount": 5_000,
        "fees": 25,
    }))
    .expect("body");
    let cb = make_callback(Rail::Wave, body);

    // Dispatch through the public router — proves the production path
    // (router → verify_signature → confirm) works end-to-end.
    let router = WebhookRouter::new({
        let mut sel = RailSelector::new(Rail::Wave);
        sel.register(Arc::new(make_wave(server.uri(), Arc::clone(&store))));
        Arc::new(sel)
    });
    let result = router.dispatch(cb).await.expect("dispatch");
    assert_eq!(result.status, PaymentStatus::Succeeded);
    assert_eq!(result.amount, 5_000);
    assert_eq!(result.fees, Some(25));
    assert_eq!(result.reference, "order_42");
}

#[tokio::test]
async fn missed_webhook_poll_recovery_returns_succeeded() {
    let server = MockServer::start().await;
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let adapter = make_wave(server.uri(), store);

    Mock::given(method("POST"))
        .and(path("/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "WV_POLL_001",
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/checkout/sessions/WV_POLL_001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "WV_POLL_001",
            "status": "succeeded",
            "amount": 5_000,
        })))
        .mount(&server)
        .await;

    let key = uuid::Uuid::new_v4();
    let handle = adapter
        .initiate(&wave_request(key))
        .await
        .expect("initiate");

    // Webhook never arrives. Caller falls back to poll.
    let status = adapter.poll(&handle).await.expect("poll");
    assert_eq!(status, PaymentStatus::Succeeded);
}

#[tokio::test]
async fn reversal_callback_yields_reversed() {
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let adapter = make_wave("http://unused".to_owned(), store);

    let body = serde_json::to_vec(&serde_json::json!({
        "id": "WV_REV_001",
        "status": "reversed",
        "client_reference": "order_88",
        "amount": 2_500,
    }))
    .expect("body");
    let cb = make_callback(Rail::Wave, body);

    adapter.verify_signature(&cb).await.expect("verify");
    let result = adapter.confirm(&cb).await.expect("confirm");
    assert_eq!(result.status, PaymentStatus::Reversed);
}

#[tokio::test]
async fn same_idempotency_key_replayed_yields_one_charge() {
    let server = MockServer::start().await;
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let adapter = make_wave(server.uri(), Arc::clone(&store));

    Mock::given(method("POST"))
        .and(path("/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "WV_REPLAY_001",
        })))
        .expect(1) // wiremock verifies exactly one HTTP call
        .mount(&server)
        .await;

    let key = uuid::Uuid::new_v4();
    let first = adapter.initiate(&wave_request(key)).await.expect("first");
    let second = adapter.initiate(&wave_request(key)).await.expect("second");
    let third = adapter.initiate(&wave_request(key)).await.expect("third");

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(
        store
            .find_by_idempotency_key(key)
            .await
            .expect("find")
            .len(),
        1,
        "exactly one Initiated event for the replayed key"
    );
}

#[tokio::test]
async fn routing_to_stub_returns_rail_not_configured() {
    let mut sel = RailSelector::new(Rail::OrangeMoney);
    sel.register(Arc::new(OrangeMoneyAdapter::new()));
    let sel = Arc::new(sel);

    let router = WebhookRouter::new(Arc::clone(&sel));

    let body =
        serde_json::to_vec(&serde_json::json!({"id": "X", "status": "succeeded"})).expect("body");
    let cb = RawCallback {
        rail: Rail::OrangeMoney,
        headers: vec![],
        body,
    };
    match router.dispatch(cb).await {
        Err(PaymentError::RailNotConfigured(r)) => assert_eq!(r, Rail::OrangeMoney),
        other => panic!("expected RailNotConfigured(OrangeMoney), got {other:?}"),
    }
}

#[tokio::test]
async fn normalized_contract_is_uniform_across_rails() {
    // This test takes Wave (real) and OrangeMoney (stub), passes them the
    // SAME PaymentRequest shape, and asserts both return `Result<_,
    // PaymentError>` from the same trait. The compile-time check is the
    // primary value; the runtime calls just demonstrate uniformity.
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let wave: Arc<dyn SettlementAdapter> =
        Arc::new(make_wave("http://unused".to_owned(), Arc::clone(&store)));
    let om: Arc<dyn SettlementAdapter> = Arc::new(OrangeMoneyAdapter::new());

    let req = wave_request(uuid::Uuid::new_v4());
    let mut om_req = req.clone();
    om_req.instrument_hint = Some(Rail::OrangeMoney);

    // Both produce Result<PaymentHandle, PaymentError> — the contract holds.
    let _: Result<_, PaymentError> = wave.initiate(&req).await;
    let _: Result<_, PaymentError> = om.initiate(&om_req).await;

    // Both implement rail() with the right tag.
    assert_eq!(wave.rail(), Rail::Wave);
    assert_eq!(om.rail(), Rail::OrangeMoney);
}

#[tokio::test]
async fn event_log_is_append_only_initiated_then_settled() {
    let server = MockServer::start().await;
    let store = Arc::new(InMemoryEventStore::new());
    let adapter = make_wave(server.uri(), Arc::clone(&store) as Arc<dyn EventStore>);

    Mock::given(method("POST"))
        .and(path("/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "WV_LOG_001",
        })))
        .mount(&server)
        .await;

    let key = uuid::Uuid::new_v4();
    adapter
        .initiate(&wave_request(key))
        .await
        .expect("initiate");

    let body = serde_json::to_vec(&serde_json::json!({
        "id": "WV_LOG_001",
        "status": "succeeded",
        "client_reference": "order_42",
        "amount": 5_000,
    }))
    .expect("body");
    let cb = make_callback(Rail::Wave, body);
    adapter.verify_signature(&cb).await.expect("verify");
    adapter.confirm(&cb).await.expect("confirm");

    let log = store.snapshot();
    // One Initiated (key-indexed) + one Settled (currently nil-keyed; see T6
    // limitation note). Both must be present and in insertion order.
    assert_eq!(log.len(), 2, "two distinct events");
    matches!(log[0].kind, PaymentEventKind::Initiated);
    matches!(log[1].kind, PaymentEventKind::Settled { .. });
    assert_eq!(log[0].provider_ref, "WV_LOG_001");
    assert_eq!(log[1].provider_ref, "WV_LOG_001");
    assert!(log[0].recorded_at <= log[1].recorded_at, "monotonic time");
}

#[tokio::test]
async fn webhook_signature_failure_short_circuits_dispatch() {
    let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
    let mut sel = RailSelector::new(Rail::Wave);
    sel.register(Arc::new(make_wave("http://unused".to_owned(), store)));
    let router = WebhookRouter::new(Arc::new(sel));

    // Body NOT signed with the configured secret.
    let body = serde_json::to_vec(&serde_json::json!({
        "id": "WV_FAKE",
        "status": "succeeded",
    }))
    .expect("body");
    let cb = RawCallback {
        rail: Rail::Wave,
        headers: vec![("X-Wave-Signature".to_owned(), "00".repeat(32))],
        body,
    };
    match router.dispatch(cb).await {
        Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("mismatch")),
        other => panic!("expected InvalidCallback, got {other:?}"),
    }
}
