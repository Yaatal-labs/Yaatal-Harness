//! Crypto adapter — **stub**. x402-style, optional, deferred.
//!
//! TASK.md §1: "Crypto is an optional, last, deferred adapter — not a launch
//! rail." TASK.md §8: "Don't invent provider endpoints" — this stub stays
//! empty until and unless a verified on-chain settlement scheme is wired.
//! The live path **must not** depend on any blockchain.

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::webhook::RawCallback;

#[derive(Default)]
pub struct CryptoAdapter;

impl CryptoAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SettlementAdapter for CryptoAdapter {
    fn rail(&self) -> Rail {
        Rail::Crypto
    }

    async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Crypto))
    }

    async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Crypto))
    }

    async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Crypto))
    }

    async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Crypto))
    }
}
