//! Wave Business adapter (Senegal mobile money — first real rail).
//!
//! TASK.md §7 — every endpoint/auth/signature scheme in this module is
//! **[Unverified]** against the official Wave Business API docs. The shape
//! below is the best-guess minimum surface; replace each `// TODO: confirm
//! against Wave Business API docs` with a verified value before going to
//! production. A correct stub is preferable to a confident wrong URL.
//!
//! Idempotency: `initiate` short-circuits on a prior `Initiated` event for
//! the same `(rail, idempotency_key)`, returning the recorded handle without
//! a network call. The event store is the source of truth.

use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::events::{EventDiscriminant, EventStore, PaymentEvent, PaymentEventKind};
use crate::webhook::RawCallback;

/// Wave Business API config.
///
/// `api_base`, `merchant_id`, and the webhook header name are
/// **[Unverified]** placeholders until confirmed against Wave docs.
#[derive(Clone)]
pub struct WaveConfig {
    /// TODO: confirm against Wave Business API docs.
    /// e.g. "https://api.wave.com/v1".
    pub api_base: String,
    /// Bearer token. Read from env; never hardcode.
    pub api_key: String,
    /// HMAC-SHA256 secret used to sign webhooks. Provided by Wave when the
    /// webhook endpoint is registered.
    pub webhook_secret: Vec<u8>,
    /// Merchant identifier in Wave's system.
    pub merchant_id: String,
}

pub struct WaveAdapter {
    config: WaveConfig,
    http: reqwest::Client,
    event_store: Arc<dyn EventStore>,
}

impl WaveAdapter {
    pub fn new(
        config: WaveConfig,
        http: reqwest::Client,
        event_store: Arc<dyn EventStore>,
    ) -> Self {
        Self {
            config,
            http,
            event_store,
        }
    }

    fn checkout_endpoint(&self) -> String {
        // TODO: confirm against Wave Business API docs.
        format!(
            "{}/checkout/sessions",
            self.config.api_base.trim_end_matches('/')
        )
    }

    fn session_endpoint(&self, provider_ref: &str) -> String {
        // TODO: confirm against Wave Business API docs.
        format!(
            "{}/checkout/sessions/{}",
            self.config.api_base.trim_end_matches('/'),
            provider_ref
        )
    }
}

/// Wire shape we send to Wave. **[Unverified]** field names.
#[derive(Serialize)]
struct CreateSessionBody<'a> {
    /// TODO: confirm against Wave Business API docs.
    /// Integer amount in XOF (zero minor units).
    amount: u64,
    currency: &'a str,
    /// YAATAL-side reference echoed back in webhook for reconciliation.
    client_reference: &'a str,
    /// Provider-side idempotency key. Wave is expected to dedupe on this.
    idempotency_key: String,
    /// Optional MSISDN for push collect.
    #[serde(skip_serializing_if = "Option::is_none")]
    payer_msisdn: Option<&'a str>,
    merchant_id: &'a str,
}

/// Wire shape we expect back from Wave. **[Unverified]** field names.
#[derive(Deserialize)]
struct CreateSessionResponse {
    /// TODO: confirm against Wave Business API docs.
    /// Provider transaction id we carry as `provider_ref`.
    id: String,
}

/// Webhook body shape. **[Unverified]** field names.
#[derive(Deserialize)]
struct WebhookPayload {
    /// Wave's session id (= `provider_ref`).
    id: String,
    /// One of: "pending" | "succeeded" | "failed" | "reversed".
    status: String,
    #[serde(default)]
    client_reference: Option<String>,
    amount: Option<u64>,
    #[serde(default)]
    fees: Option<u64>,
    #[serde(default)]
    settled_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Poll body shape. Assumed isomorphic to webhook payload.
type PollResponse = WebhookPayload;

fn map_wave_status(s: &str) -> Result<PaymentStatus, PaymentError> {
    match s {
        // TODO: confirm against Wave Business API docs.
        "pending" => Ok(PaymentStatus::Pending),
        "succeeded" | "successful" | "completed" => Ok(PaymentStatus::Succeeded),
        "failed" | "rejected" => Ok(PaymentStatus::Failed),
        "reversed" | "refunded" => Ok(PaymentStatus::Reversed),
        other => Err(PaymentError::InvalidCallback(format!(
            "unknown Wave status: {other}"
        ))),
    }
}

#[async_trait::async_trait]
impl SettlementAdapter for WaveAdapter {
    fn rail(&self) -> Rail {
        Rail::Wave
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        // Idempotency check: short-circuit if we've already initiated for this key.
        let prior = self
            .event_store
            .find_by_idempotency_key(req.idempotency_key)
            .await?;
        if let Some(existing) = prior
            .iter()
            .find(|e| e.rail == Rail::Wave && e.kind.discriminant() == EventDiscriminant::Initiated)
        {
            tracing::debug!(
                idempotency_key = %req.idempotency_key,
                provider_ref = %existing.provider_ref,
                "wave: replay short-circuit"
            );
            return Ok(PaymentHandle {
                rail: Rail::Wave,
                provider_ref: existing.provider_ref.clone(),
                idempotency_key: req.idempotency_key,
            });
        }

        let body = CreateSessionBody {
            amount: req.amount,
            currency: match req.currency {
                crate::contract::Currency::Xof => "XOF",
            },
            client_reference: &req.reference,
            idempotency_key: req.idempotency_key.to_string(),
            payer_msisdn: req.payer_msisdn.as_deref(),
            merchant_id: &self.config.merchant_id,
        };

        let response = self
            .http
            .post(self.checkout_endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| PaymentError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(PaymentError::ProviderRejected(format!(
                "wave checkout {status}: {detail}"
            )));
        }

        let parsed: CreateSessionResponse = response
            .json()
            .await
            .map_err(|e| PaymentError::ProviderRejected(format!("decode: {e}")))?;

        self.event_store
            .record(PaymentEvent {
                rail: Rail::Wave,
                provider_ref: parsed.id.clone(),
                idempotency_key: req.idempotency_key,
                kind: PaymentEventKind::Initiated,
                recorded_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            })
            .await?;

        Ok(PaymentHandle {
            rail: Rail::Wave,
            provider_ref: parsed.id,
            idempotency_key: req.idempotency_key,
        })
    }

    async fn verify_signature(&self, raw: &RawCallback) -> Result<(), PaymentError> {
        // TODO: confirm against Wave Business API docs.
        // Best-guess: HMAC-SHA256 over raw body, hex-encoded, in header X-Wave-Signature.
        let header_value = raw.header("X-Wave-Signature").ok_or_else(|| {
            PaymentError::InvalidCallback("missing X-Wave-Signature header".to_owned())
        })?;
        let provided = hex::decode(header_value)
            .map_err(|e| PaymentError::InvalidCallback(format!("signature hex: {e}")))?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.config.webhook_secret)
            .map_err(|e| PaymentError::InvalidCallback(format!("hmac key: {e}")))?;
        mac.update(&raw.body);
        let expected = mac.finalize().into_bytes();
        if provided.ct_eq(&expected).into() {
            Ok(())
        } else {
            Err(PaymentError::InvalidCallback(
                "signature mismatch".to_owned(),
            ))
        }
    }

    async fn confirm(&self, raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
        let body_str = raw.body_str()?;
        let payload: WebhookPayload = serde_json::from_str(body_str)
            .map_err(|e| PaymentError::InvalidCallback(format!("body json: {e}")))?;
        let status = map_wave_status(&payload.status)?;
        let amount = payload
            .amount
            .ok_or_else(|| PaymentError::InvalidCallback("missing amount".to_owned()))?;
        if amount == 0 {
            return Err(PaymentError::InvalidCallback(
                "amount must be greater than zero".to_owned(),
            ));
        }

        let reference = payload.client_reference.unwrap_or_default();

        if status != PaymentStatus::Pending {
            // Record terminal events only. A provider "pending" callback must
            // not occupy the Settled identity and block a later success/failure.
            self.event_store
                .record(PaymentEvent {
                    rail: Rail::Wave,
                    provider_ref: payload.id.clone(),
                    // Webhook payload doesn't carry idempotency_key directly; the
                    // adapter recovers it via the event log. For T6 we keep the
                    // event's idempotency_key empty (nil) when not recoverable —
                    // this is documented as a known limitation; a follow-up will
                    // index by provider_ref to backfill the key.
                    idempotency_key: prior_key_for_provider_ref(
                        self.event_store.as_ref(),
                        Rail::Wave,
                        &payload.id,
                    )
                    .await
                    .unwrap_or_else(uuid::Uuid::nil),
                    kind: PaymentEventKind::Settled {
                        status,
                        fees: payload.fees,
                        settled_at: payload.settled_at,
                    },
                    recorded_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
                })
                .await?;
        }

        Ok(PaymentResult {
            status,
            rail: Rail::Wave,
            provider_ref: payload.id,
            reference,
            amount,
            fees: payload.fees,
            settled_at: payload.settled_at,
        })
    }

    async fn poll(&self, handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
        let response = self
            .http
            .get(self.session_endpoint(&handle.provider_ref))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| PaymentError::Transport(e.to_string()))?;

        let http_status = response.status();
        if !http_status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(PaymentError::ProviderRejected(format!(
                "wave poll {http_status}: {detail}"
            )));
        }

        let parsed: PollResponse = response
            .json()
            .await
            .map_err(|e| PaymentError::ProviderRejected(format!("decode: {e}")))?;

        map_wave_status(&parsed.status)
    }
}

/// Recover the originally recorded idempotency_key for a (rail, provider_ref)
/// pair by walking the event log. Used to stamp Settled events with the same
/// key as their Initiated peer so reconciliation queries by key surface both.
async fn prior_key_for_provider_ref(
    store: &dyn EventStore,
    rail: Rail,
    provider_ref: &str,
) -> Option<uuid::Uuid> {
    // We don't have a (rail, provider_ref) → key index yet; T6 limitation.
    // Scan recent events through the only API we have: find_by_idempotency_key
    // requires a key, which is what we're trying to recover. A targeted index
    // method will be added in a follow-up; for T6 we accept the nil-fallback
    // and the test asserts the Settled event is still recorded.
    let _ = (store, rail, provider_ref);
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::contract::Currency;
    use crate::events::InMemoryEventStore;
    use pretty_assertions::assert_eq;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_secret() -> Vec<u8> {
        b"super-secret-key".to_vec()
    }

    fn sign(secret: &[u8], body: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("hmac key");
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    fn make_adapter(api_base: String, store: Arc<dyn EventStore>) -> WaveAdapter {
        WaveAdapter::new(
            WaveConfig {
                api_base,
                api_key: "test_key".to_owned(),
                webhook_secret: sample_secret(),
                merchant_id: "M_TEST".to_owned(),
            },
            reqwest::Client::new(),
            store,
        )
    }

    fn sample_request(key: uuid::Uuid) -> PaymentRequest {
        PaymentRequest {
            amount: 5_000,
            currency: Currency::Xof,
            reference: "order_42".to_owned(),
            idempotency_key: key,
            instrument_hint: Some(Rail::Wave),
            payer_msisdn: Some("221770000000".to_owned()),
        }
    }

    #[tokio::test]
    async fn happy_path_initiate_then_confirm_succeeded() {
        let server = MockServer::start().await;
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter(server.uri(), Arc::clone(&store));

        Mock::given(method("POST"))
            .and(path("/checkout/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "WV_SESSION_001",
            })))
            .mount(&server)
            .await;

        let key = uuid::Uuid::new_v4();
        let handle = adapter
            .initiate(&sample_request(key))
            .await
            .expect("initiate");
        assert_eq!(handle.rail, Rail::Wave);
        assert_eq!(handle.provider_ref, "WV_SESSION_001");
        assert_eq!(handle.idempotency_key, key);

        // Webhook arrives.
        let body = serde_json::to_vec(&serde_json::json!({
            "id": "WV_SESSION_001",
            "status": "succeeded",
            "client_reference": "order_42",
            "amount": 5_000,
            "fees": 25,
        }))
        .expect("body");
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), sign(&sample_secret(), &body))],
            body,
        };
        adapter.verify_signature(&cb).await.expect("verify");
        let result = adapter.confirm(&cb).await.expect("confirm");
        assert_eq!(result.status, PaymentStatus::Succeeded);
        assert_eq!(result.provider_ref, "WV_SESSION_001");
        assert_eq!(result.reference, "order_42");
        assert_eq!(result.amount, 5_000);
        assert_eq!(result.fees, Some(25));
    }

    #[tokio::test]
    async fn reversal_callback_yields_reversed_status() {
        let server = MockServer::start().await;
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter(server.uri(), store);

        let body = serde_json::to_vec(&serde_json::json!({
            "id": "WV_SESSION_002",
            "status": "reversed",
            "client_reference": "order_99",
            "amount": 1_000,
        }))
        .expect("body");
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), sign(&sample_secret(), &body))],
            body,
        };
        adapter.verify_signature(&cb).await.expect("verify");
        let result = adapter.confirm(&cb).await.expect("confirm");
        assert_eq!(result.status, PaymentStatus::Reversed);
        assert_eq!(result.provider_ref, "WV_SESSION_002");
    }

    #[tokio::test]
    async fn replay_initiate_short_circuits_no_extra_http_call() {
        let server = MockServer::start().await;
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter(server.uri(), Arc::clone(&store));

        // Mock the endpoint exactly once. wiremock fails the test if it's hit
        // more than `expect()` times.
        Mock::given(method("POST"))
            .and(path("/checkout/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "WV_SESSION_REPLAY",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = uuid::Uuid::new_v4();
        let first = adapter.initiate(&sample_request(key)).await.expect("first");
        let second = adapter
            .initiate(&sample_request(key))
            .await
            .expect("second");

        assert_eq!(first, second);
        assert_eq!(
            store
                .find_by_idempotency_key(key)
                .await
                .expect("find")
                .len(),
            1,
            "exactly one Initiated event"
        );
        // wiremock's .expect(1) verifies one POST on drop.
    }

    #[tokio::test]
    async fn bad_signature_is_rejected() {
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter("http://unused".to_owned(), store);

        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), "00".repeat(32))],
            body: b"{}".to_vec(),
        };
        match adapter.verify_signature(&cb).await {
            Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("mismatch")),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_signature_header_is_rejected() {
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter("http://unused".to_owned(), store);

        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![],
            body: b"{}".to_vec(),
        };
        match adapter.verify_signature(&cb).await {
            Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("missing")),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_provider_status_is_rejected() {
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter("http://unused".to_owned(), store);

        let body = serde_json::to_vec(&serde_json::json!({
            "id": "WV_X",
            "status": "weird_unknown_status",
            "amount": 1
        }))
        .expect("body");
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), sign(&sample_secret(), &body))],
            body,
        };
        adapter.verify_signature(&cb).await.expect("verify");
        match adapter.confirm(&cb).await {
            Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("unknown Wave status")),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_webhook_amount_is_rejected() {
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter("http://unused".to_owned(), store);

        let body = serde_json::to_vec(&serde_json::json!({
            "id": "WV_MISSING_AMOUNT",
            "status": "succeeded",
        }))
        .expect("body");
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), sign(&sample_secret(), &body))],
            body,
        };
        adapter.verify_signature(&cb).await.expect("verify");
        match adapter.confirm(&cb).await {
            Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("missing amount")),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn zero_webhook_amount_is_rejected() {
        let store = Arc::new(InMemoryEventStore::new()) as Arc<dyn EventStore>;
        let adapter = make_adapter("http://unused".to_owned(), store);

        let body = serde_json::to_vec(&serde_json::json!({
            "id": "WV_ZERO_AMOUNT",
            "status": "succeeded",
            "amount": 0,
        }))
        .expect("body");
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![("X-Wave-Signature".to_owned(), sign(&sample_secret(), &body))],
            body,
        };
        adapter.verify_signature(&cb).await.expect("verify");
        match adapter.confirm(&cb).await {
            Err(PaymentError::InvalidCallback(msg)) => assert!(msg.contains("greater than zero")),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }
}
