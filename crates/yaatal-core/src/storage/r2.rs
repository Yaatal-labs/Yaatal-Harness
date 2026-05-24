//! Cloudflare R2-backed storage store stub (`feature = "r2"`).
//!
//! `R2Store` implements [`StorageDispatcher`] **only** for [`Public`]-tagged
//! values.  Attempting to call `put` or `get` with a [`Sovereign`]- or
//! [`Operational`]-tagged value is a **compile-time error**:
//!
//! ```rust,compile_fail
//! # #[cfg(feature = "r2")]
//! # {
//! use yaatal_core::policy::{Sovereign, Tagged};
//! use yaatal_core::storage::r2::R2Store;
//! let store = R2Store::new();
//! let secret: Tagged<String, Sovereign> = Tagged::new("classified".into());
//! // This must not compile — R2Store only implements StorageDispatcher<_, Public>.
//! let _ = store.put("k", secret);
//! # }
//! ```
//!
//! The actual Cloudflare R2 client (aws-sdk-s3 R2-compat) is not yet wired;
//! all methods return a descriptive [`StorageError::Backend`] for now.

use crate::policy::{Public, Tagged};
use crate::storage::dispatch::{StorageDispatcher, StorageError};

// ---------------------------------------------------------------------------
// R2Store
// ---------------------------------------------------------------------------

/// Cloudflare R2-backed edge store.
///
/// Only stores values tagged [`Public`].  Sovereign and operational data
/// cannot be passed to this type — the `impl` constrains `S = Public`,
/// so the compiler rejects any other sensitivity at the call site.
///
/// # Wiring
/// The R2 client (aws-sdk-s3 R2-compatible endpoint) is not yet wired.
/// All calls return [`StorageError::Backend`] until a follow-up lane adds the
/// client.  The `aws-sdk-s3` crate is intentionally not listed in
/// `Cargo.toml` yet — it is a heavy dependency that should only be added
/// when the client is ready to be used.
pub struct R2Store {
    // placeholder — wire the aws-sdk-s3 R2 client in a follow-up lane.
}

impl R2Store {
    /// Create a new `R2Store` stub.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for R2Store {
    fn default() -> Self {
        Self::new()
    }
}

// R2 accepts ONLY Public-tagged values.
// The `S = Public` constraint here is what prevents Sovereign/Operational data
// from ever being passed to this backend at the type level.
impl<T: Send + Sync> StorageDispatcher<T, Public> for R2Store {
    async fn put(&self, _key: &str, _value: Tagged<T, Public>) -> Result<(), StorageError> {
        Err(StorageError::Backend(
            "R2 client not wired; follow-up".into(),
        ))
    }

    async fn get(&self, _key: &str) -> Result<Option<Tagged<T, Public>>, StorageError> {
        Err(StorageError::Backend(
            "R2 client not wired; follow-up".into(),
        ))
    }
}
