//! `StorageDispatcher` trait and `StorageError` type.
//!
//! The trait is generic over `S: SensitivityTag`; type-level enforcement lives
//! in the *impls*, not here.  Backend implementations decide which sensitivity
//! levels they accept by constraining the `S` parameter in their `impl` blocks.

use crate::policy::{Sensitivity, SensitivityTag, Tagged};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that may be returned by a [`StorageDispatcher`] backend.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The backend returned an error (network, I/O, serialisation, …).
    #[error("backend: {0}")]
    Backend(String),

    /// A `get` call succeeded but no record exists for the requested key.
    #[error("not found")]
    NotFound,

    /// The requested write is forbidden: the given sensitivity level is not
    /// permitted by this backend.
    #[error("forbidden: {0:?} cannot be stored in {1}")]
    Forbidden(Sensitivity, &'static str),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// A storage backend that can store and retrieve values tagged with a
/// compile-time sensitivity level.
///
/// # Type-level enforcement
/// Implementors constrain the `S` type parameter in their `impl` blocks.
/// For example, an R2 store only implements `StorageDispatcher<T, Public>`,
/// so passing a `Tagged<T, Sovereign>` to it is a **compile-time error**.
///
/// # Object safety
/// This trait is **not** object-safe because it has async methods and is
/// generic over `T` and `S`.  Use static dispatch (generics / `impl Trait`).
pub trait StorageDispatcher<T, S: SensitivityTag>: Send + Sync {
    /// Store `value` under `key`.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] if the underlying backend fails, or
    /// [`StorageError::Forbidden`] if the sensitivity level is not permitted.
    fn put(
        &self,
        key: &str,
        value: Tagged<T, S>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Retrieve the value stored under `key`, if any.
    ///
    /// # Errors
    /// Returns [`StorageError::Backend`] if the underlying backend fails.
    /// Returns `Ok(None)` when the key is not found (rather than
    /// [`StorageError::NotFound`], which is reserved for contexts where the
    /// absence of a value is itself an error).
    fn get(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<Tagged<T, S>>, StorageError>> + Send;
}

// We need std::future::Future in scope for the RPITIT bounds above.
use std::future::Future;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::policy::{Public, Sovereign};
    use crate::storage::memory::MockStore;

    // -----------------------------------------------------------------------
    // put_then_get_roundtrip_on_mock_store
    // -----------------------------------------------------------------------

    /// Store a `Sovereign`-tagged `String` in a `MockStore<Sovereign>`, then
    /// retrieve it and assert the value and sensitivity level are preserved.
    #[tokio::test]
    async fn put_then_get_roundtrip_on_mock_store() {
        let store: MockStore<String, Sovereign> = MockStore::new();
        let key = "profiles/alice";
        let secret = String::from("classified-payload");
        let tagged = Tagged::<String, Sovereign>::new(secret.clone());

        store.put(key, tagged).await.expect("put must succeed");

        let fetched = store
            .get(key)
            .await
            .expect("get must succeed")
            .expect("value must be present");

        assert_eq!(fetched.level(), Sensitivity::Sovereign);
        assert_eq!(fetched.into_inner(), secret);
    }

    // -----------------------------------------------------------------------
    // mock_store_isolates_by_sensitivity
    // -----------------------------------------------------------------------

    /// `MockStore<Sovereign>` and `MockStore<Public>` are distinct types.
    /// This test acts as a compile-time witness: if the two stores shared the
    /// same monomorphisation, swapping them would compile — but it does not.
    #[tokio::test]
    async fn mock_store_isolates_by_sensitivity() {
        let sovereign_store: MockStore<String, Sovereign> = MockStore::new();
        let public_store: MockStore<String, Public> = MockStore::new();

        // Both stores accept their own sensitivity level.
        sovereign_store
            .put("s/key", Tagged::<String, Sovereign>::new("sovereign-val".into()))
            .await
            .expect("sovereign store put");

        public_store
            .put("p/key", Tagged::<String, Public>::new("public-val".into()))
            .await
            .expect("public store put");

        // Each store only exposes what was written to it.
        let from_sovereign = sovereign_store
            .get("s/key")
            .await
            .expect("get from sovereign store")
            .expect("value must exist");
        assert_eq!(from_sovereign.level(), Sensitivity::Sovereign);

        let from_public = public_store
            .get("p/key")
            .await
            .expect("get from public store")
            .expect("value must exist");
        assert_eq!(from_public.level(), Sensitivity::Public);

        // Key written to sovereign store is absent from public store.
        let cross_lookup = public_store
            .get("s/key")
            .await
            .expect("cross-store get must not error");
        assert!(
            cross_lookup.is_none(),
            "sovereign key must not be visible in public store"
        );
    }
}
