//! Postgres-backed storage store stub.
//!
//! `PostgresStore` implements [`StorageDispatcher`] for **any** sensitivity
//! level — sovereign data is permitted here.  The actual sea-orm connection
//! pool will be wired in a Lane 1 follow-up; for now, every call returns a
//! `Backend` error describing the missing wiring.

use crate::policy::{SensitivityTag, Tagged};
use crate::storage::dispatch::{StorageDispatcher, StorageError};

// ---------------------------------------------------------------------------
// PostgresStore
// ---------------------------------------------------------------------------

/// Postgres-backed storage backend.
///
/// Accepts data tagged with any [`SensitivityTag`] — including [`Sovereign`]
/// and [`Operational`] data, which must never go to an edge store.
///
/// # Wiring
/// The sea-orm connection pool is not yet wired.  The `put` and `get` methods
/// return [`StorageError::Backend`] with a descriptive message until Lane 1
/// supplies a real [`ConnectOptions`].
///
/// [`Sovereign`]: crate::policy::Sovereign
/// [`ConnectOptions`]: sea_orm::ConnectOptions
pub struct PostgresStore {
    // placeholder — wire the sea-orm ConnectOptions in Lane 1 follow-up.
}

impl PostgresStore {
    /// Create a new `PostgresStore` stub.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PostgresStore {
    fn default() -> Self {
        Self::new()
    }
}

// Postgres accepts any sensitivity level.
impl<T: Send + Sync, S: SensitivityTag> StorageDispatcher<T, S> for PostgresStore {
    async fn put(&self, _key: &str, _value: Tagged<T, S>) -> Result<(), StorageError> {
        Err(StorageError::Backend(
            "Postgres store not wired; Lane 1 follow-up".into(),
        ))
    }

    async fn get(&self, _key: &str) -> Result<Option<Tagged<T, S>>, StorageError> {
        Err(StorageError::Backend(
            "Postgres store not wired; Lane 1 follow-up".into(),
        ))
    }
}
