//! Payments service — wires `yaatal_payments` adapters into the Loco-rs app.
//!
//! One `PaymentsService` is constructed at boot time from environment variables
//! and stored via `axum::Extension`. Handlers receive it via `Extension`.
//!
//! Only Wave is wired as a real adapter. The in-memory `EventStore` is used for
//! now; Lane 5b will swap in the Postgres-backed store.

use std::sync::Arc;

use yaatal_payments::{
    adapters::wave::{WaveAdapter, WaveConfig},
    contract::{PaymentHandle, PaymentRequest, PaymentStatus, Rail},
    error::PaymentError,
    events::{EventStore, InMemoryEventStore},
    selector::RailSelector,
    webhook::{RawCallback, WebhookRouter},
    PaymentResult,
};

/// Shared payments service held in `axum::Extension`.
pub struct PaymentsService {
    selector: Arc<RailSelector>,
    router: WebhookRouter,
    #[allow(dead_code)]
    event_store: Arc<dyn EventStore>,
}

impl PaymentsService {
    /// Construct from environment variables.
    ///
    /// Required env vars:
    /// - `WAVE_API_BASE`
    /// - `WAVE_API_KEY`
    /// - `WAVE_WEBHOOK_SECRET` (hex-encoded bytes)
    /// - `WAVE_MERCHANT_ID`
    ///
    /// Returns `Err(PaymentError::Transport("missing WAVE_* config"))` if any var is absent.
    pub fn from_env() -> Result<Self, PaymentError> {
        let api_base = std::env::var("WAVE_API_BASE")
            .map_err(|_| PaymentError::Transport("missing WAVE_* config".to_owned()))?;
        let api_key = std::env::var("WAVE_API_KEY")
            .map_err(|_| PaymentError::Transport("missing WAVE_* config".to_owned()))?;
        let secret_hex = std::env::var("WAVE_WEBHOOK_SECRET")
            .map_err(|_| PaymentError::Transport("missing WAVE_* config".to_owned()))?;
        let merchant_id = std::env::var("WAVE_MERCHANT_ID")
            .map_err(|_| PaymentError::Transport("missing WAVE_* config".to_owned()))?;

        let webhook_secret = hex::decode(secret_hex.trim())
            .map_err(|e| PaymentError::Transport(format!("WAVE_WEBHOOK_SECRET hex: {e}")))?;

        let event_store: Arc<dyn EventStore> = Arc::new(InMemoryEventStore::new());

        let wave_adapter = WaveAdapter::new(
            WaveConfig {
                api_base,
                api_key,
                webhook_secret,
                merchant_id,
            },
            reqwest::Client::new(),
            Arc::clone(&event_store),
        );

        let mut selector = RailSelector::new(Rail::Wave);
        selector.register(Arc::new(wave_adapter));
        let selector = Arc::new(selector);
        let router = WebhookRouter::new(Arc::clone(&selector));

        Ok(Self {
            selector,
            router,
            event_store,
        })
    }

    /// Initiate a payment. Returns a pending handle.
    pub async fn initiate(&self, req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        let adapter = self.selector.pick(req).ok_or_else(|| {
            PaymentError::RailNotConfigured(req.instrument_hint.unwrap_or(Rail::Wave))
        })?;
        adapter.initiate(req).await
    }

    /// Dispatch a raw webhook callback to the appropriate adapter.
    pub async fn dispatch_webhook(&self, raw: RawCallback) -> Result<PaymentResult, PaymentError> {
        self.router.dispatch(raw).await
    }

    /// Poll a payment's current status from the provider.
    pub async fn poll(
        &self,
        rail: Rail,
        provider_ref: &str,
    ) -> Result<PaymentStatus, PaymentError> {
        let adapter = self
            .selector
            .adapter_for_rail(rail)
            .ok_or(PaymentError::RailNotConfigured(rail))?;
        let handle = PaymentHandle {
            rail,
            provider_ref: provider_ref.to_owned(),
            // idempotency_key is not used by poll; nil is a safe placeholder.
            idempotency_key: uuid::Uuid::nil(),
        };
        adapter.poll(&handle).await
    }
}
