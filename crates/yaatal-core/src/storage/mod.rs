//! Storage dispatcher — type-level residency enforcement (§8.2, Lane 6).
//!
//! # Design
//! The core abstraction is the [`StorageDispatcher`] trait, which is generic over
//! a value type `T` and a [`SensitivityTag`] `S`.  Concrete backend impls
//! restrict which `S` they accept:
//!
//! - [`PostgresStore`] — accepts any sensitivity (sovereign data is allowed here).
//! - [`R2Store`] — accepts only [`Public`] (gated by `#[cfg(feature = "r2")]`).
//! - [`MockStore`] — accepts any sensitivity; backed by an in-memory `HashMap`.
//!
//! The type-level boundary is enforced at the impl, not the trait, so the
//! compiler rejects attempts to call `R2Store::put` with a [`Tagged<T, Sovereign>`]
//! at compile time rather than at runtime.

pub mod dispatch;
pub mod memory;
pub mod postgres;

#[cfg(feature = "r2")]
pub mod r2;

pub use dispatch::{StorageDispatcher, StorageError};
pub use memory::MockStore;
pub use postgres::PostgresStore;

#[cfg(feature = "r2")]
pub use r2::R2Store;
