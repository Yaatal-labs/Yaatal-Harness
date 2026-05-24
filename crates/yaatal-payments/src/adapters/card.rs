//! Card adapter — **stub** until a PSP partnership and merchant account
//! are in place. Card support is explicitly later than mobile money in the
//! product order (TASK.md §4: `Card // via a PSP, later`).

use crate::adapter::SettlementAdapter;
use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::webhook::RawCallback;

#[derive(Default)]
pub struct CardAdapter;

impl CardAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SettlementAdapter for CardAdapter {
    fn rail(&self) -> Rail {
        Rail::Card
    }

    async fn initiate(&self, _req: &PaymentRequest) -> Result<PaymentHandle, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Card))
    }

    async fn verify_signature(&self, _raw: &RawCallback) -> Result<(), PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Card))
    }

    async fn confirm(&self, _raw: &RawCallback) -> Result<PaymentResult, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Card))
    }

    async fn poll(&self, _handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError> {
        Err(PaymentError::RailNotConfigured(Rail::Card))
    }
}
