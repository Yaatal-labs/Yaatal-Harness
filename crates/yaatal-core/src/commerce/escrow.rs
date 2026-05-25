//! Escrow state machine for BOBO orders.
//!
//! Models the lifecycle of funds held in escrow while a BOBO order is
//! in flight. All state changes must go through `transition` so the
//! legal-transition table is the single source of truth.
//!
//! Legal transitions:
//!
//! ```text
//! Held      ──Release──►  Released
//! Held      ──Dispute──►  Disputed
//! Held      ──Refund───►  Refunded
//! Released  ──Settle───►  Settled
//! Disputed  ──Release──►  Released   (merchant-favour resolution)
//! Disputed  ──Refund───►  Refunded   (buyer-favour resolution)
//! ```
//!
//! All other combinations return `EscrowError::IllegalTransition`.

use thiserror::Error;

/// Current state of the escrow account for a given order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowState {
    /// Funds are held; delivery not yet confirmed.
    Held,
    /// Funds released to merchant; awaiting settlement run.
    Released,
    /// Funds settled into the merchant account.
    Settled,
    /// Order is under dispute; funds frozen.
    Disputed,
    /// Funds refunded to the buyer.
    Refunded,
}

impl std::fmt::Display for EscrowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscrowState::Held => f.write_str("held"),
            EscrowState::Released => f.write_str("released"),
            EscrowState::Settled => f.write_str("settled"),
            EscrowState::Disputed => f.write_str("disputed"),
            EscrowState::Refunded => f.write_str("refunded"),
        }
    }
}

/// Events that can trigger an escrow state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowTransition {
    /// Merchant confirms delivery; release funds.
    Release,
    /// Buyer or system initiates settlement run.
    Settle,
    /// A party raises a dispute.
    Dispute,
    /// Funds are returned to the buyer (buyer-favour resolution or reversal).
    Refund,
}

impl std::fmt::Display for EscrowTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscrowTransition::Release => f.write_str("Release"),
            EscrowTransition::Settle => f.write_str("Settle"),
            EscrowTransition::Dispute => f.write_str("Dispute"),
            EscrowTransition::Refund => f.write_str("Refund"),
        }
    }
}

/// Errors from the escrow state machine.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EscrowError {
    #[error("illegal escrow transition: cannot apply {transition} to state {from}")]
    IllegalTransition {
        from: EscrowState,
        transition: EscrowTransition,
    },
}

/// Apply `ev` to the `current` escrow state.
///
/// Returns the new `EscrowState` on success, or
/// `EscrowError::IllegalTransition` if the combination is not legal.
///
/// This is a pure function — all side-effects (DB `UPDATE ... WHERE state =
/// $expected RETURNING *`) are the caller's responsibility.
pub fn transition(
    current: EscrowState,
    ev: EscrowTransition,
) -> Result<EscrowState, EscrowError> {
    match (current, ev) {
        // ── Legal transitions ──────────────────────────────────────────────
        (EscrowState::Held, EscrowTransition::Release) => Ok(EscrowState::Released),
        (EscrowState::Held, EscrowTransition::Dispute) => Ok(EscrowState::Disputed),
        (EscrowState::Held, EscrowTransition::Refund) => Ok(EscrowState::Refunded),
        (EscrowState::Released, EscrowTransition::Settle) => Ok(EscrowState::Settled),
        // Dispute resolution paths:
        (EscrowState::Disputed, EscrowTransition::Release) => Ok(EscrowState::Released),
        (EscrowState::Disputed, EscrowTransition::Refund) => Ok(EscrowState::Refunded),
        // ── Everything else is illegal ─────────────────────────────────────
        (from, transition) => Err(EscrowError::IllegalTransition { from, transition }),
    }
}
