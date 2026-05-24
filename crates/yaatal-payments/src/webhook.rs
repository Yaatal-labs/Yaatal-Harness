//! Provider webhook intake.
//!
//! T3 ships the `RawCallback` shape only — the dispatch router and
//! signature-verification machinery land in T5.

use crate::contract::Rail;

/// Raw, unparsed provider callback.
///
/// Adapters are responsible for verifying the signature, decoding the body,
/// and producing a normalized `PaymentResult` via `SettlementAdapter::confirm`.
#[derive(Debug, Clone)]
pub struct RawCallback {
    pub rail: Rail,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
