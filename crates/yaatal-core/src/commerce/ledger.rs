//! BOBO ledger types — construction helpers for the `bobo_ledger` table.
//!
//! The ledger is append-only; a DB trigger prevents UPDATE/DELETE.
//! `LedgerEntry` mirrors the table columns and is used for constructing
//! new rows before persistence. Reading rows uses sea-orm entities
//! (generated separately, or via raw query).

use chrono::{DateTime, Utc};

/// Credit increases a balance; Debit decreases it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerDirection {
    Credit,
    Debit,
}

impl LedgerDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            LedgerDirection::Credit => "credit",
            LedgerDirection::Debit => "debit",
        }
    }
}

/// Semantic classification of a ledger entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKind {
    /// Incoming payment from the buyer.
    Payment,
    /// Full or partial refund to the buyer.
    Refund,
    /// Platform fee charged on settlement.
    Fee,
    /// Manual or system correction.
    Adjustment,
}

impl LedgerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LedgerKind::Payment => "payment",
            LedgerKind::Refund => "refund",
            LedgerKind::Fee => "fee",
            LedgerKind::Adjustment => "adjustment",
        }
    }
}

/// A pending ledger entry ready for insertion.
///
/// `id` and `created_at` are assigned by the database; they are `None`
/// until the row is persisted and returned.
#[derive(Debug, Clone)]
pub struct LedgerEntry {
    pub id: Option<i64>,
    pub order_id: i64,
    pub direction: LedgerDirection,
    /// Integer XOF — always positive (sign carried by `direction`).
    pub amount_xof: i64,
    pub kind: LedgerKind,
    /// Provider reference or internal trace id; optional.
    pub ref_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

impl LedgerEntry {
    /// Construct a new (unpersisted) payment credit entry.
    pub fn payment_credit(order_id: i64, amount_xof: i64, ref_id: Option<String>) -> Self {
        Self {
            id: None,
            order_id,
            direction: LedgerDirection::Credit,
            amount_xof,
            kind: LedgerKind::Payment,
            ref_id,
            created_at: None,
        }
    }

    /// Construct a new (unpersisted) refund debit entry.
    pub fn refund_debit(order_id: i64, amount_xof: i64, ref_id: Option<String>) -> Self {
        Self {
            id: None,
            order_id,
            direction: LedgerDirection::Debit,
            amount_xof,
            kind: LedgerKind::Refund,
            ref_id,
            created_at: None,
        }
    }

    /// Construct a new (unpersisted) fee debit entry.
    pub fn fee_debit(order_id: i64, amount_xof: i64) -> Self {
        Self {
            id: None,
            order_id,
            direction: LedgerDirection::Debit,
            amount_xof,
            kind: LedgerKind::Fee,
            ref_id: None,
            created_at: None,
        }
    }
}
