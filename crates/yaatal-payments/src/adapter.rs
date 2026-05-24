//! The `SettlementAdapter` trait.
//!
//! Every adapter speaks the normalized contract from `contract.rs`. Adapters
//! absorb provider-specific request, callback, and status vocabularies — they
//! never leak provider shapes upward.

use crate::contract::{PaymentHandle, PaymentRequest, PaymentResult, PaymentStatus, Rail};
use crate::error::PaymentError;
use crate::webhook::RawCallback;

#[async_trait::async_trait]
pub trait SettlementAdapter: Send + Sync {
    fn rail(&self) -> Rail;

    /// Initiate a collection. Returns a pending handle. MUST NOT block on
    /// final settlement — truth arrives via `confirm`/`poll`.
    async fn initiate(&self, req: &PaymentRequest) -> Result<PaymentHandle, PaymentError>;

    /// Normalize a provider callback into a result.
    /// MUST be idempotent on `(rail, provider_ref, idempotency_key)`.
    async fn confirm(&self, raw: &RawCallback) -> Result<PaymentResult, PaymentError>;

    /// Fallback when a webhook is missed.
    async fn poll(&self, handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError>;
}
