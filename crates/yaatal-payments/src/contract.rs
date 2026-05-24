//! Normalized payment contract — every adapter speaks this shape.
//!
//! Callers must never branch on which rail was used. The adapter absorbs all
//! provider-specific request, callback, and status vocabularies.

use serde::{Deserialize, Serialize};

/// Settlement rails. Stable identifiers used in logs + reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rail {
    Wave,
    OrangeMoney,
    FreeMoney,
    /// Via a PSP, later.
    Card,
    /// x402-style, optional, deferred.
    Crypto,
}

/// Settlement currency.
///
/// XOF (West African CFA franc) has **zero minor units** — store amount as an
/// integer number of CFA francs, never multiply by 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Currency {
    Xof,
}

/// A request to collect a payment.
///
/// `idempotency_key` is caller-supplied and replay-safe: the same key replayed
/// returns the originally recorded handle, never a second charge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentRequest {
    /// Integer XOF (no cents).
    pub amount: u64,
    pub currency: Currency,
    /// YAATAL-side intent/order id.
    pub reference: String,
    pub idempotency_key: uuid::Uuid,
    /// Caller may force a rail; else the selector decides.
    pub instrument_hint: Option<Rail>,
    /// MSISDN for mobile-money push collect.
    pub payer_msisdn: Option<String>,
}

/// Normalized terminal-state vocabulary.
///
/// Mobile money DOES reverse — the x402 "no chargebacks" assumption does not
/// transfer. Treat `Reversed` as a real terminal state, not an error path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
    Reversed,
}

/// Pending handle returned by `SettlementAdapter::initiate`.
///
/// Truth about final state arrives later via `confirm` (webhook) or `poll`
/// (fallback). Never derive settlement from this handle alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentHandle {
    pub rail: Rail,
    /// Provider-side transaction id.
    pub provider_ref: String,
    pub idempotency_key: uuid::Uuid,
}

/// Terminal result of a payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentResult {
    pub status: PaymentStatus,
    pub rail: Rail,
    pub provider_ref: String,
    pub reference: String,
    pub amount: u64,
    /// Provider-reported, when available.
    pub fees: Option<u64>,
    pub settled_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use pretty_assertions::assert_eq;

    fn sample_request() -> PaymentRequest {
        PaymentRequest {
            amount: 5_000,
            currency: Currency::Xof,
            reference: "order_42".to_owned(),
            idempotency_key: uuid::Uuid::nil(),
            instrument_hint: Some(Rail::Wave),
            payer_msisdn: Some("221770000000".to_owned()),
        }
    }

    fn sample_handle() -> PaymentHandle {
        PaymentHandle {
            rail: Rail::Wave,
            provider_ref: "WV_TXN_001".to_owned(),
            idempotency_key: uuid::Uuid::nil(),
        }
    }

    fn sample_result() -> PaymentResult {
        PaymentResult {
            status: PaymentStatus::Succeeded,
            rail: Rail::Wave,
            provider_ref: "WV_TXN_001".to_owned(),
            reference: "order_42".to_owned(),
            amount: 5_000,
            fees: Some(50),
            settled_at: chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0),
        }
    }

    #[test]
    fn payment_request_roundtrips_through_json() {
        let original = sample_request();
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: PaymentRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }

    #[test]
    fn payment_handle_roundtrips_through_json() {
        let original = sample_handle();
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: PaymentHandle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }

    #[test]
    fn payment_result_roundtrips_through_json() {
        let original = sample_result();
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: PaymentResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }

    #[test]
    fn payment_status_roundtrips_through_json() {
        for status in [
            PaymentStatus::Pending,
            PaymentStatus::Succeeded,
            PaymentStatus::Failed,
            PaymentStatus::Reversed,
        ] {
            let json = serde_json::to_string(&status).expect("serialize");
            let parsed: PaymentStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn rail_roundtrips_through_json() {
        for rail in [
            Rail::Wave,
            Rail::OrangeMoney,
            Rail::FreeMoney,
            Rail::Card,
            Rail::Crypto,
        ] {
            let json = serde_json::to_string(&rail).expect("serialize");
            let parsed: Rail = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(rail, parsed);
        }
    }
}
