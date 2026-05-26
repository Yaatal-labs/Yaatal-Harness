//! `payment_events` store — append-only log, idempotent on the natural key.
//!
//! Identity is `(rail, provider_ref, idempotency_key, kind_discriminant)`.
//! Recording the same identity twice is a no-op that returns the originally
//! stored event — "same key twice yields one logical charge" (TASK.md §2.4).
//!
//! T4 ships only the in-memory backing and the trait shape. A Postgres-backed
//! impl lives in `yaatal-core::commerce::payment_events_postgres` (Lane 5b).

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::contract::{PaymentStatus, Rail};
use crate::error::PaymentError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentEventKind {
    /// A `SettlementAdapter::initiate` was successfully invoked and the
    /// provider returned a handle. Re-recording with the same identity is a
    /// no-op (replay).
    Initiated,
    /// A terminal state was observed via `confirm` or `poll`.
    Settled {
        status: PaymentStatus,
        fees: Option<u64>,
        settled_at: Option<chrono::DateTime<chrono::Utc>>,
    },
}

impl PaymentEventKind {
    /// Stable discriminant used in the dedup key. Two events with the same
    /// `(rail, provider_ref, idempotency_key)` but different discriminants are
    /// distinct log entries (e.g., `Initiated` + `Settled`).
    pub fn discriminant(&self) -> EventDiscriminant {
        match self {
            PaymentEventKind::Initiated => EventDiscriminant::Initiated,
            PaymentEventKind::Settled { .. } => EventDiscriminant::Settled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventDiscriminant {
    Initiated,
    Settled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub rail: Rail,
    pub provider_ref: String,
    pub idempotency_key: uuid::Uuid,
    pub kind: PaymentEventKind,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

impl PaymentEvent {
    pub fn identity(&self) -> EventIdentity {
        EventIdentity {
            rail: self.rail,
            provider_ref: self.provider_ref.clone(),
            idempotency_key: self.idempotency_key,
            discriminant: self.kind.discriminant(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventIdentity {
    pub rail: Rail,
    pub provider_ref: String,
    pub idempotency_key: uuid::Uuid,
    pub discriminant: EventDiscriminant,
}

#[async_trait::async_trait]
pub trait EventStore: Send + Sync {
    /// Append the event. If an event with the same identity already exists,
    /// return the originally stored event without modifying the log
    /// (replay-safe). The store assigns `recorded_at` itself; the value
    /// supplied on the input event is ignored.
    async fn record(&self, draft: PaymentEvent) -> Result<PaymentEvent, PaymentError>;

    /// All events recorded under this idempotency key, in insertion order.
    /// Adapters call this before invoking a provider to short-circuit replays.
    async fn find_by_idempotency_key(
        &self,
        key: uuid::Uuid,
    ) -> Result<Vec<PaymentEvent>, PaymentError>;
}

/// In-memory `EventStore` for tests and the default dev binary. Replace with a
/// Postgres-backed implementation in production via Lane 5b.
#[derive(Default)]
pub struct InMemoryEventStore {
    inner: Mutex<InMemoryInner>,
}

#[derive(Default)]
struct InMemoryInner {
    log: Vec<PaymentEvent>,
    by_identity: HashMap<EventIdentity, usize>,
}

impl InMemoryEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the log in insertion order. Useful for reconciliation tests.
    pub fn snapshot(&self) -> Vec<PaymentEvent> {
        match self.inner.lock() {
            Ok(guard) => guard.log.clone(),
            Err(poisoned) => poisoned.into_inner().log.clone(),
        }
    }
}

#[async_trait::async_trait]
impl EventStore for InMemoryEventStore {
    async fn record(&self, draft: PaymentEvent) -> Result<PaymentEvent, PaymentError> {
        let identity = draft.identity();
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(&idx) = guard.by_identity.get(&identity) {
            let existing = guard.log.get(idx).cloned().ok_or_else(|| {
                PaymentError::Transport(
                    "internal: event index points outside log bounds".to_owned(),
                )
            })?;
            return Ok(existing);
        }

        let stored = PaymentEvent {
            recorded_at: chrono::Utc::now(),
            ..draft
        };
        let idx = guard.log.len();
        guard.log.push(stored.clone());
        guard.by_identity.insert(identity, idx);
        Ok(stored)
    }

    async fn find_by_idempotency_key(
        &self,
        key: uuid::Uuid,
    ) -> Result<Vec<PaymentEvent>, PaymentError> {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        Ok(guard
            .log
            .iter()
            .filter(|e| e.idempotency_key == key)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use pretty_assertions::assert_eq;

    fn make_event(rail: Rail, provider_ref: &str, key: uuid::Uuid) -> PaymentEvent {
        PaymentEvent {
            rail,
            provider_ref: provider_ref.to_owned(),
            idempotency_key: key,
            kind: PaymentEventKind::Initiated,
            recorded_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        }
    }

    #[tokio::test]
    async fn same_identity_twice_yields_one_log_entry() {
        let store = InMemoryEventStore::new();
        let key = uuid::Uuid::new_v4();

        let first = store
            .record(make_event(Rail::Wave, "WV_001", key))
            .await
            .expect("first record");
        let second = store
            .record(make_event(Rail::Wave, "WV_001", key))
            .await
            .expect("second record");

        assert_eq!(first, second, "replay must return the original event");
        assert_eq!(store.snapshot().len(), 1, "log has exactly one entry");
    }

    #[tokio::test]
    async fn different_identities_are_distinct_log_entries() {
        let store = InMemoryEventStore::new();
        let key_a = uuid::Uuid::new_v4();
        let key_b = uuid::Uuid::new_v4();

        store
            .record(make_event(Rail::Wave, "WV_001", key_a))
            .await
            .expect("a");
        store
            .record(make_event(Rail::Wave, "WV_002", key_a))
            .await
            .expect("different provider_ref");
        store
            .record(make_event(Rail::OrangeMoney, "OM_001", key_a))
            .await
            .expect("different rail");
        store
            .record(make_event(Rail::Wave, "WV_001", key_b))
            .await
            .expect("different key");

        assert_eq!(store.snapshot().len(), 4);
    }

    #[tokio::test]
    async fn initiated_and_settled_are_distinct_for_same_triple() {
        let store = InMemoryEventStore::new();
        let key = uuid::Uuid::new_v4();

        let mut init = make_event(Rail::Wave, "WV_001", key);
        init.kind = PaymentEventKind::Initiated;
        store.record(init).await.expect("init");

        let mut settled = make_event(Rail::Wave, "WV_001", key);
        settled.kind = PaymentEventKind::Settled {
            status: PaymentStatus::Succeeded,
            fees: Some(25),
            settled_at: chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0),
        };
        store.record(settled).await.expect("settled");

        assert_eq!(
            store.snapshot().len(),
            2,
            "Initiated and Settled coexist for the same triple"
        );
    }

    #[tokio::test]
    async fn find_returns_all_events_for_key() {
        let store = InMemoryEventStore::new();
        let key = uuid::Uuid::new_v4();
        let other_key = uuid::Uuid::new_v4();

        store
            .record(make_event(Rail::Wave, "WV_001", key))
            .await
            .expect("a");
        store
            .record({
                let mut e = make_event(Rail::Wave, "WV_001", key);
                e.kind = PaymentEventKind::Settled {
                    status: PaymentStatus::Succeeded,
                    fees: None,
                    settled_at: None,
                };
                e
            })
            .await
            .expect("b");
        store
            .record(make_event(Rail::Wave, "WV_999", other_key))
            .await
            .expect("c");

        let found = store.find_by_idempotency_key(key).await.expect("find");
        assert_eq!(found.len(), 2, "two events for the queried key");
        assert_eq!(
            store
                .find_by_idempotency_key(other_key)
                .await
                .expect("find other")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn store_overrides_caller_supplied_timestamp() {
        let store = InMemoryEventStore::new();
        let key = uuid::Uuid::new_v4();

        let draft = make_event(Rail::Wave, "WV_001", key); // UNIX_EPOCH
        let stored = store.record(draft).await.expect("record");

        assert_ne!(
            stored.recorded_at,
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            "store stamps its own time, not the caller's"
        );
    }
}
