//! Provider webhook intake + dispatch.
//!
//! `RawCallback` is the carrier — rail tag, headers, body bytes. The
//! `WebhookRouter` looks up the right adapter by rail, calls
//! `verify_signature`, then `confirm`. Every error path returns a typed
//! `PaymentError`; malformed input never panics.

use std::sync::Arc;

use crate::contract::{PaymentResult, Rail};
use crate::error::PaymentError;
use crate::selector::RailSelector;

/// Raw, unparsed provider callback.
///
/// Adapters are responsible for parsing the body, verifying the signature, and
/// producing a normalized `PaymentResult` via `SettlementAdapter::confirm`.
#[derive(Debug, Clone)]
pub struct RawCallback {
    pub rail: Rail,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawCallback {
    /// Case-insensitive header lookup. Returns the first match.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// UTF-8 view of the body, if valid. Adapters that expect JSON typically
    /// call this first; otherwise they parse `body` as bytes directly.
    pub fn body_str(&self) -> Result<&str, PaymentError> {
        std::str::from_utf8(&self.body)
            .map_err(|e| PaymentError::InvalidCallback(format!("body is not utf-8: {e}")))
    }
}

/// Routes a `RawCallback` to its rail's adapter, verifies the signature, and
/// invokes `confirm`. Reuses `RailSelector` for adapter lookup so the same
/// registration is the source of truth for both initiation and webhook
/// dispatch.
pub struct WebhookRouter {
    selector: Arc<RailSelector>,
}

impl WebhookRouter {
    pub fn new(selector: Arc<RailSelector>) -> Self {
        Self { selector }
    }

    /// Dispatch the callback. Returns the normalized result on success,
    /// a typed error otherwise. Never panics on malformed input.
    pub async fn dispatch(&self, raw: RawCallback) -> Result<PaymentResult, PaymentError> {
        let adapter = self
            .selector
            .adapter_for_rail(raw.rail)
            .ok_or(PaymentError::RailNotConfigured(raw.rail))?;
        adapter.verify_signature(&raw).await?;
        adapter.confirm(&raw).await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::adapter::SettlementAdapter;
    use crate::contract::{PaymentHandle, PaymentRequest, PaymentStatus};
    use pretty_assertions::assert_eq;

    struct AlwaysOkAdapter {
        rail: Rail,
        provider_ref: &'static str,
    }

    #[async_trait::async_trait]
    impl SettlementAdapter for AlwaysOkAdapter {
        fn rail(&self) -> Rail {
            self.rail
        }

        async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
            Err(PaymentError::Transport(
                "not used in webhook test".to_owned(),
            ))
        }

        async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
            Ok(())
        }

        async fn confirm(&self, raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
            Ok(PaymentResult {
                status: PaymentStatus::Succeeded,
                rail: self.rail,
                provider_ref: self.provider_ref.to_owned(),
                reference: raw.header("X-Reference").unwrap_or("ref").to_owned(),
                amount: 1_000,
                fees: None,
                settled_at: None,
            })
        }

        async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
            Err(PaymentError::Transport("not used".to_owned()))
        }
    }

    struct BadSignatureAdapter {
        rail: Rail,
    }

    #[async_trait::async_trait]
    impl SettlementAdapter for BadSignatureAdapter {
        fn rail(&self) -> Rail {
            self.rail
        }

        async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
            Err(PaymentError::Transport("not used".to_owned()))
        }

        async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
            Err(PaymentError::InvalidCallback("bad sig".to_owned()))
        }

        async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
            panic!("confirm must not be called after signature verification failure");
        }

        async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
            Err(PaymentError::Transport("not used".to_owned()))
        }
    }

    fn raw(rail: Rail) -> RawCallback {
        RawCallback {
            rail,
            headers: vec![
                ("Content-Type".to_owned(), "application/json".to_owned()),
                ("X-Reference".to_owned(), "order_42".to_owned()),
            ],
            body: b"{}".to_vec(),
        }
    }

    fn selector_with(adapter: Arc<dyn SettlementAdapter>) -> Arc<RailSelector> {
        let mut sel = RailSelector::new(adapter.rail());
        sel.register(adapter);
        Arc::new(sel)
    }

    #[tokio::test]
    async fn happy_path_returns_result() {
        let router = WebhookRouter::new(selector_with(Arc::new(AlwaysOkAdapter {
            rail: Rail::Wave,
            provider_ref: "WV_001",
        })));
        let result = router.dispatch(raw(Rail::Wave)).await.expect("dispatch");
        assert_eq!(result.status, PaymentStatus::Succeeded);
        assert_eq!(result.provider_ref, "WV_001");
        assert_eq!(result.reference, "order_42");
    }

    #[tokio::test]
    async fn unregistered_rail_returns_rail_not_configured() {
        let router = WebhookRouter::new(selector_with(Arc::new(AlwaysOkAdapter {
            rail: Rail::Wave,
            provider_ref: "WV_001",
        })));
        let err = router
            .dispatch(raw(Rail::OrangeMoney))
            .await
            .expect_err("unregistered rail");
        match err {
            PaymentError::RailNotConfigured(rail) => assert_eq!(rail, Rail::OrangeMoney),
            other => panic!("expected RailNotConfigured, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn signature_failure_short_circuits_before_confirm() {
        let router = WebhookRouter::new(selector_with(Arc::new(BadSignatureAdapter {
            rail: Rail::Wave,
        })));
        let err = router
            .dispatch(raw(Rail::Wave))
            .await
            .expect_err("bad signature");
        match err {
            PaymentError::InvalidCallback(msg) => assert_eq!(msg, "bad sig"),
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_body_does_not_panic_router() {
        // The router itself doesn't parse the body. This test confirms that
        // even an empty body / unusual headers reaches the adapter without
        // the router panicking on the way down.
        let router = WebhookRouter::new(selector_with(Arc::new(AlwaysOkAdapter {
            rail: Rail::Wave,
            provider_ref: "WV_001",
        })));
        let mut callback = raw(Rail::Wave);
        callback.body.clear();
        callback.headers.clear();
        let result = router.dispatch(callback).await.expect("dispatch");
        assert_eq!(result.reference, "ref"); // fallback when X-Reference is absent
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: vec![
                ("X-Wave-Signature".to_owned(), "abc123".to_owned()),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: Vec::new(),
        };
        assert_eq!(cb.header("x-wave-signature"), Some("abc123"));
        assert_eq!(cb.header("X-WAVE-SIGNATURE"), Some("abc123"));
        assert_eq!(cb.header("content-type"), Some("application/json"));
        assert_eq!(cb.header("missing"), None);
    }

    #[test]
    fn body_str_rejects_non_utf8() {
        let cb = RawCallback {
            rail: Rail::Wave,
            headers: Vec::new(),
            body: vec![0xFF, 0xFE, 0xFD],
        };
        match cb.body_str() {
            Err(PaymentError::InvalidCallback(_)) => {}
            other => panic!("expected InvalidCallback, got {other:?}"),
        }
    }
}
