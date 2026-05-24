//! Payment errors. One enum, exhaustive at the boundary.

use crate::contract::Rail;

#[derive(Debug, thiserror::Error)]
pub enum PaymentError {
    #[error("rail not configured: {0:?}")]
    RailNotConfigured(Rail),
    #[error("duplicate idempotency key with conflicting payload")]
    IdempotencyConflict,
    #[error("provider rejected: {0}")]
    ProviderRejected(String),
    #[error("provider unreachable: {0}")]
    Transport(String),
    #[error("invalid callback: {0}")]
    InvalidCallback(String),
}
