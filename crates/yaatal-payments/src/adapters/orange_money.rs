//! Orange Money adapter — **stub** until merchant/partner API access is granted
//! (Orange developer/partner program onboarding required, TASK.md §7).

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::webhook::RawCallback;

#[derive(Default)]
pub struct OrangeMoneyAdapter;

impl OrangeMoneyAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SettlementAdapter for OrangeMoneyAdapter {
    fn rail(&self) -> Rail {
        Rail::OrangeMoney
    }

    async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::OrangeMoney))
    }

    async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::OrangeMoney))
    }

    async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::OrangeMoney))
    }

    async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::OrangeMoney))
    }
}
