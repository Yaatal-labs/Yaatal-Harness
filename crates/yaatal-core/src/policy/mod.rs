//! Policy types for the Yaatal residency boundary.
//!
//! This module defines the canonical `Sensitivity` enum and the `Tagged<T, S>`
//! marker-type pattern that enforces storage residency at the type level:
//! sovereign data stays in Diamniadio Postgres, only `Public` may mirror to R2.

pub mod sensitivity;

pub use sensitivity::{Operational, Public, Sensitivity, SensitivityTag, Sovereign, Tagged};
