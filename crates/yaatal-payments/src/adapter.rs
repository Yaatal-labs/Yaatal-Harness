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

    /// Verify the provider's signature over the raw callback. Each adapter
    /// owns the scheme (HMAC header, JWS, mTLS, etc.). MUST return
    /// `PaymentError::InvalidCallback` on any verification failure — never
    /// panic on malformed input.
    async fn verify_signature(&self, raw: &RawCallback) -> Result<(), PaymentError>;

    /// Normalize a provider callback into a result. Assumes
    /// `verify_signature` has already returned `Ok`.
    /// MUST be idempotent on `(rail, provider_ref, idempotency_key)`.
    async fn confirm(&self, raw: &RawCallback) -> Result<PaymentResult, PaymentError>;

    /// Fallback when a webhook is missed.
    async fn poll(&self, handle: &PaymentHandle) -> Result<PaymentStatus, PaymentError>;
}
