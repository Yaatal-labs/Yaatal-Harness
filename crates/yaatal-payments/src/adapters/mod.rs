//! Per-rail `SettlementAdapter` implementations.
//!
//! - [`wave`] — first real rail (mobile money, Senegal).
//! - Other rails ship as stubs returning `RailNotConfigured` in T7.

pub mod wave;

pub use wave::{WaveAdapter, WaveConfig};
