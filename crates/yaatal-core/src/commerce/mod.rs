//! BOBO commerce domain — escrow state machine, ledger, KYC, and the
//! Postgres-backed payment-events bridge.

pub mod escrow;
pub mod kyc;
pub mod ledger;
pub mod payment_events_postgres;

pub use escrow::{EscrowError, EscrowState, EscrowTransition};
pub use kyc::{KycProfile, KycStatus};
pub use ledger::{LedgerDirection, LedgerEntry, LedgerKind};
pub use payment_events_postgres::PostgresEventStore;
