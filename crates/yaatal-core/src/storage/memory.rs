//! In-memory `MockStore` — generic over `T` and `S: SensitivityTag`.
//!
//! Useful for tests and for bootstrapping call sites before a real backend is
//! wired.  `MockStore` accepts any sensitivity level; the restriction to a
//! single `S` in a given instance is a compile-time guarantee that values of
//! different sensitivity levels are always stored in separate maps.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::policy::{SensitivityTag, Tagged};
use crate::storage::dispatch::{StorageDispatcher, StorageError};

// ---------------------------------------------------------------------------
// MockStore
// ---------------------------------------------------------------------------

/// An in-memory storage backend backed by a `Mutex<HashMap<String, T>>`.
///
/// The type parameter `S: SensitivityTag` binds the store to a single
/// sensitivity level at compile time so that values of different sensitivity
/// levels are never silently mixed.
///
/// # Constraints
/// `T` must be `Clone + Send + Sync + 'static`.  `Clone` is required because
/// `get` returns a new `Tagged<T, S>` rather than a reference into the map.
pub struct MockStore<T: Clone + Send + Sync + 'static, S: SensitivityTag> {
    inner: Mutex<HashMap<String, T>>,
    _tag: std::marker::PhantomData<fn() -> S>,
}

impl<T: Clone + Send + Sync + 'static, S: SensitivityTag> MockStore<T, S> {
    /// Create an empty `MockStore`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            _tag: std::marker::PhantomData,
        }
    }
}

impl<T: Clone + Send + Sync + 'static, S: SensitivityTag> Default for MockStore<T, S> {
    fn default() -> Self {
        Self::new()
    }
}

// MockStore is Send because Mutex<HashMap<String, T>> is Send when T: Send.
// MockStore is Sync because Mutex provides interior mutability with sync access.
// SAFETY: the PhantomData is variance-only and does not affect Send/Sync.
unsafe impl<T: Clone + Send + Sync + 'static, S: SensitivityTag> Send for MockStore<T, S> {}
unsafe impl<T: Clone + Send + Sync + 'static, S: SensitivityTag> Sync for MockStore<T, S> {}

impl<T: Clone + Send + Sync + 'static, S: SensitivityTag> StorageDispatcher<T, S>
    for MockStore<T, S>
{
    async fn put(&self, key: &str, value: Tagged<T, S>) -> Result<(), StorageError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|e| StorageError::Backend(format!("mutex poisoned: {e}")))?;
        map.insert(key.to_owned(), value.into_inner());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Tagged<T, S>>, StorageError> {
        let map = self
            .inner
            .lock()
            .map_err(|e| StorageError::Backend(format!("mutex poisoned: {e}")))?;
        Ok(map.get(key).cloned().map(Tagged::new))
    }
}
