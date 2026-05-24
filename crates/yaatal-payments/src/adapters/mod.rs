//! Per-rail `SettlementAdapter` implementations.
//!
//! - [`wave`] — first real rail (mobile money, Senegal).
//! - [`orange_money`], [`free_money`], [`card`], [`crypto`] — stubs returning
//!   `RailNotConfigured` until merchant accounts and verified endpoints land
//!   (TASK.md §7).

pub mod card;
pub mod crypto;
pub mod free_money;
pub mod orange_money;
pub mod wave;

pub use card::CardAdapter;
pub use crypto::CryptoAdapter;
pub use free_money::FreeMoneyAdapter;
pub use orange_money::OrangeMoneyAdapter;
pub use wave::{WaveAdapter, WaveConfig};

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use std::sync::Arc;

    use super::*;
    use crate::adapter::SettlementAdapter;
    use crate::contract::{Currency, PaymentRequest, Rail};
    use crate::error::PaymentError;
    use crate::selector::RailSelector;
    use crate::webhook::RawCallback;

    fn sample_request(rail: Rail) -> PaymentRequest {
        PaymentRequest {
            amount: 1_000,
            currency: Currency::Xof,
            reference: "ref".to_owned(),
            idempotency_key: uuid::Uuid::nil(),
            instrument_hint: Some(rail),
            payer_msisdn: None,
        }
    }

    fn empty_callback(rail: Rail) -> RawCallback {
        RawCallback {
            rail,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    async fn assert_all_methods_return_rail_not_configured(adapter: Arc<dyn SettlementAdapter>) {
        let rail = adapter.rail();
        let req = sample_request(rail);
        let cb = empty_callback(rail);
        let handle = crate::contract::PaymentHandle {
            rail,
            provider_ref: "stub_ref".to_owned(),
            idempotency_key: uuid::Uuid::nil(),
        };

        for err in [
            adapter.initiate(&req).await.expect_err("initiate"),
            adapter.verify_signature(&cb).await.expect_err("verify"),
            adapter.confirm(&cb).await.expect_err("confirm"),
            adapter.poll(&handle).await.expect_err("poll"),
        ] {
            match err {
                PaymentError::RailNotConfigured(r) => assert_eq!(r, rail),
                other => panic!("expected RailNotConfigured({rail:?}), got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn orange_money_stub_fails_cleanly() {
        assert_all_methods_return_rail_not_configured(Arc::new(OrangeMoneyAdapter::new())).await;
    }

    #[tokio::test]
    async fn free_money_stub_fails_cleanly() {
        assert_all_methods_return_rail_not_configured(Arc::new(FreeMoneyAdapter::new())).await;
    }

    #[tokio::test]
    async fn card_stub_fails_cleanly() {
        assert_all_methods_return_rail_not_configured(Arc::new(CardAdapter::new())).await;
    }

    #[tokio::test]
    async fn crypto_stub_fails_cleanly() {
        assert_all_methods_return_rail_not_configured(Arc::new(CryptoAdapter::new())).await;
    }

    #[tokio::test]
    async fn selector_routes_to_each_stub_by_hint() {
        // DoD: "selector can route to them; they fail cleanly."
        let mut sel = RailSelector::new(Rail::Wave);
        sel.register(Arc::new(OrangeMoneyAdapter::new()));
        sel.register(Arc::new(FreeMoneyAdapter::new()));
        sel.register(Arc::new(CardAdapter::new()));
        sel.register(Arc::new(CryptoAdapter::new()));

        for rail in [
            Rail::OrangeMoney,
            Rail::FreeMoney,
            Rail::Card,
            Rail::Crypto,
        ] {
            let picked = sel
                .pick(&sample_request(rail))
                .unwrap_or_else(|| panic!("rail {rail:?} not registered"));
            assert_eq!(picked.rail(), rail);
            match picked.initiate(&sample_request(rail)).await {
                Err(PaymentError::RailNotConfigured(r)) => assert_eq!(r, rail),
                other => panic!("expected RailNotConfigured({rail:?}), got {other:?}"),
            }
        }
    }
}
