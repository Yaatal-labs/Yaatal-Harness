//! Rail selection.
//!
//! Picks an adapter by `PaymentRequest::instrument_hint` if set; otherwise
//! falls back to the configured default rail. If the chosen rail has no
//! registered adapter, returns `None` so the caller can map it to a
//! `PaymentError::RailNotConfigured`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentRequest, Rail};

pub struct RailSelector {
    default: Rail,
    adapters: HashMap<Rail, Arc<dyn SettlementAdapter>>,
}

impl RailSelector {
    pub fn new(default: Rail) -> Self {
        Self {
            default,
            adapters: HashMap::new(),
        }
    }

    /// Register an adapter under its declared rail. Replaces any previously
    /// registered adapter for the same rail.
    pub fn register(&mut self, adapter: Arc<dyn SettlementAdapter>) {
        let rail = adapter.rail();
        self.adapters.insert(rail, adapter);
    }

    /// Resolve to the rail this request will use. Honors `instrument_hint`
    /// over the default. Does not check registration.
    pub fn resolve_rail(&self, req: &PaymentRequest) -> Rail {
        req.instrument_hint.unwrap_or(self.default)
    }

    /// Pick the adapter for this request. `None` means the resolved rail
    /// has no registered adapter.
    pub fn pick(&self, req: &PaymentRequest) -> Option<Arc<dyn SettlementAdapter>> {
        let rail = self.resolve_rail(req);
        self.adapters.get(&rail).map(Arc::clone)
    }

    /// Look up an adapter purely by rail tag. Used by the webhook router,
    /// which receives a `RawCallback` already tagged with its rail.
    pub fn adapter_for_rail(&self, rail: Rail) -> Option<Arc<dyn SettlementAdapter>> {
        self.adapters.get(&rail).map(Arc::clone)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::contract::{Currency, PaymentHandle, PaymentResult, PaymentStatus};
    use crate::error::PaymentError;
    use crate::webhook::RawCallback;
    use pretty_assertions::assert_eq;

    struct MockAdapter {
        rail: Rail,
        marker: &'static str,
    }

    #[async_trait::async_trait]
    impl SettlementAdapter for MockAdapter {
        fn rail(&self) -> Rail {
            self.rail
        }

        async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
            Ok(PaymentHandle {
                rail: self.rail,
                provider_ref: self.marker.to_owned(),
                idempotency_key: uuid::Uuid::nil(),
            })
        }

        async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
            Ok(())
        }

        async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
            Err(PaymentError::InvalidCallback("mock".to_owned()))
        }

        async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
            Ok(PaymentStatus::Pending)
        }
    }

    fn req(hint: Option<Rail>) -> PaymentRequest {
        PaymentRequest {
            amount: 1_000,
            currency: Currency::Xof,
            reference: "ref".to_owned(),
            idempotency_key: uuid::Uuid::nil(),
            instrument_hint: hint,
            payer_msisdn: None,
        }
    }

    #[test]
    fn hint_overrides_default() {
        let mut sel = RailSelector::new(Rail::OrangeMoney);
        sel.register(Arc::new(MockAdapter {
            rail: Rail::Wave,
            marker: "wave",
        }));
        sel.register(Arc::new(MockAdapter {
            rail: Rail::OrangeMoney,
            marker: "om",
        }));

        assert_eq!(sel.resolve_rail(&req(Some(Rail::Wave))), Rail::Wave);
        let picked = sel.pick(&req(Some(Rail::Wave))).expect("wave registered");
        assert_eq!(picked.rail(), Rail::Wave);
    }

    #[test]
    fn default_used_when_no_hint() {
        let mut sel = RailSelector::new(Rail::Wave);
        sel.register(Arc::new(MockAdapter {
            rail: Rail::Wave,
            marker: "wave",
        }));

        assert_eq!(sel.resolve_rail(&req(None)), Rail::Wave);
        let picked = sel.pick(&req(None)).expect("default wave");
        assert_eq!(picked.rail(), Rail::Wave);
    }

    #[test]
    fn unregistered_rail_returns_none() {
        let sel = RailSelector::new(Rail::FreeMoney);
        assert!(sel.pick(&req(None)).is_none());
        assert!(sel.pick(&req(Some(Rail::Card))).is_none());
    }

    #[test]
    fn register_replaces_previous_adapter() {
        let mut sel = RailSelector::new(Rail::Wave);
        sel.register(Arc::new(MockAdapter {
            rail: Rail::Wave,
            marker: "first",
        }));
        sel.register(Arc::new(MockAdapter {
            rail: Rail::Wave,
            marker: "second",
        }));

        let picked = sel.pick(&req(None)).expect("wave registered");
        let req = req(None);
        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt")
            .block_on(picked.initiate(&req))
            .expect("initiate");
        assert_eq!(handle.provider_ref, "second");
    }
}
