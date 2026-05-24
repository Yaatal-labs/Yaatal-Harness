//! Free Money adapter — **stub** until a usable API is confirmed
//! (may require direct BD, TASK.md §7).

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::webhook::RawCallback;

#[derive(Default)]
pub struct FreeMoneyAdapter;

impl FreeMoneyAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SettlementAdapter for FreeMoneyAdapter {
    fn rail(&self) -> Rail {
        Rail::FreeMoney
    }

    async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::FreeMoney))
    }

    async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::FreeMoney))
    }

    async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::FreeMoney))
    }

    async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::FreeMoney))
    }
}
