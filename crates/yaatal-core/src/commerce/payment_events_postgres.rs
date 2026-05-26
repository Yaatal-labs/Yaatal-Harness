//! Postgres-backed `EventStore` implementation.
//!
//! Persists `yaatal_payments::events::PaymentEvent` into the
//! `bobo_payment_intents` table (created by migration
//! `m20260615_000005_bobo_payment_intents`).
//!
//! The `record()` method uses `INSERT ... ON CONFLICT (idempotency_key) DO
//! NOTHING` followed by a `SELECT` to guarantee idempotency on replay — same
//! key twice always returns the originally stored event.
//!
//! For `Settled` events the row is UPDATEd (status + updated_at) using the
//! `(rail, provider_ref, idempotency_key)` triple as the lookup key.
//!
//! `find_by_idempotency_key` selects all rows for a given key, ordered by
//! `created_at`, and synthesizes a `PaymentEvent` for each.

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};

use yaatal_payments::contract::{PaymentStatus, Rail};
use yaatal_payments::error::PaymentError;
use yaatal_payments::events::{EventStore, PaymentEvent, PaymentEventKind};

/// Postgres-backed event store. Wraps a sea-orm `DatabaseConnection`.
///
/// This type is the bridge between the storage-agnostic `yaatal-payments`
/// crate and the `bobo_payment_intents` table defined in Lane 5b's
/// migration. It implements `EventStore` so `WaveAdapter` (and future
/// adapters) can swap it in for `InMemoryEventStore` without any adapter
/// code changes.
pub struct PostgresEventStore {
    db: DatabaseConnection,
}

impl PostgresEventStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Convert a database rail text value back to `Rail`.
fn rail_from_str(s: &str) -> Result<Rail, PaymentError> {
    match s {
        "Wave" => Ok(Rail::Wave),
        "OrangeMoney" => Ok(Rail::OrangeMoney),
        "FreeMoney" => Ok(Rail::FreeMoney),
        "Card" => Ok(Rail::Card),
        "Crypto" => Ok(Rail::Crypto),
        other => Err(PaymentError::InvalidCallback(format!(
            "unknown rail in DB: {other}"
        ))),
    }
}

/// Convert `Rail` to its text representation stored in `bobo_payment_intents.rail`.
fn rail_to_str(rail: Rail) -> &'static str {
    match rail {
        Rail::Wave => "Wave",
        Rail::OrangeMoney => "OrangeMoney",
        Rail::FreeMoney => "FreeMoney",
        Rail::Card => "Card",
        Rail::Crypto => "Crypto",
    }
}

/// Convert `PaymentStatus` to the text value in `bobo_payment_intents.status`.
fn status_to_str(status: PaymentStatus) -> &'static str {
    match status {
        PaymentStatus::Pending => "pending",
        PaymentStatus::Succeeded => "succeeded",
        PaymentStatus::Failed => "failed",
        PaymentStatus::Reversed => "reversed",
    }
}

/// Convert DB status text back to `PaymentStatus`.
fn status_from_str(s: &str) -> Result<PaymentStatus, PaymentError> {
    match s {
        "pending" => Ok(PaymentStatus::Pending),
        "succeeded" => Ok(PaymentStatus::Succeeded),
        "failed" => Ok(PaymentStatus::Failed),
        "reversed" => Ok(PaymentStatus::Reversed),
        other => Err(PaymentError::InvalidCallback(format!(
            "unknown status in DB: {other}"
        ))),
    }
}

#[async_trait::async_trait]
impl EventStore for PostgresEventStore {
    /// Persist `draft` to `bobo_payment_intents`.
    ///
    /// For `Initiated` events:
    /// - INSERT with status = 'pending', ON CONFLICT (idempotency_key) DO NOTHING.
    /// - SELECT the canonical row and return it (idempotent on replay).
    ///
    /// For `Settled` events:
    /// - UPDATE the matching row (by `(rail, provider_ref, idempotency_key)`)
    ///   with the new status and `updated_at = now()`.
    /// - SELECT and return the updated row.
    async fn record(&self, draft: PaymentEvent) -> Result<PaymentEvent, PaymentError> {
        match &draft.kind {
            PaymentEventKind::Initiated => {
                // Attempt to insert; ignore if the key already exists.
                // NOTE: `order_id` is required by the FK but is not part of the
                // `PaymentEvent` type in yaatal-payments (storage-agnostic).
                // We use 0 as a sentinel that callers replace before real
                // production use. A future refactor will thread order_id through
                // the event struct or accept it as a side-channel parameter.
                let sql = "
                    INSERT INTO bobo_payment_intents
                        (order_id, rail, provider_ref, idempotency_key, status, amount_xof)
                    VALUES (0, $1, $2, $3, 'pending', 0)
                    ON CONFLICT (idempotency_key) DO NOTHING
                ";
                self.db
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Postgres,
                        sql,
                        vec![
                            Value::String(Some(Box::new(rail_to_str(draft.rail).to_owned()))),
                            Value::String(Some(Box::new(draft.provider_ref.clone()))),
                            Value::Uuid(Some(Box::new(draft.idempotency_key))),
                        ],
                    ))
                    .await
                    .map_err(|e| PaymentError::Transport(e.to_string()))?;

                // SELECT the canonical row (either just-inserted or pre-existing).
                self.fetch_by_idempotency_key_and_rail(
                    draft.idempotency_key,
                    draft.rail,
                    &draft.provider_ref,
                )
                .await
            }

            PaymentEventKind::Settled {
                status,
                fees: _,
                settled_at: _,
            } => {
                let status_str = status_to_str(*status);
                let sql = "
                    UPDATE bobo_payment_intents
                    SET    status     = $1,
                           updated_at = now()
                    WHERE  rail             = $2
                      AND  provider_ref     = $3
                      AND  idempotency_key  = $4
                ";
                self.db
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Postgres,
                        sql,
                        vec![
                            Value::String(Some(Box::new(status_str.to_owned()))),
                            Value::String(Some(Box::new(rail_to_str(draft.rail).to_owned()))),
                            Value::String(Some(Box::new(draft.provider_ref.clone()))),
                            Value::Uuid(Some(Box::new(draft.idempotency_key))),
                        ],
                    ))
                    .await
                    .map_err(|e| PaymentError::Transport(e.to_string()))?;

                self.fetch_by_idempotency_key_and_rail(
                    draft.idempotency_key,
                    draft.rail,
                    &draft.provider_ref,
                )
                .await
            }
        }
    }

    /// Return all events stored under `key`, in insertion order.
    async fn find_by_idempotency_key(
        &self,
        key: uuid::Uuid,
    ) -> Result<Vec<PaymentEvent>, PaymentError> {
        let sql = "
            SELECT rail, provider_ref, idempotency_key, status, created_at
            FROM   bobo_payment_intents
            WHERE  idempotency_key = $1
            ORDER BY created_at
        ";
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                vec![Value::Uuid(Some(Box::new(key)))],
            ))
            .await
            .map_err(|e| PaymentError::Transport(e.to_string()))?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let rail_str: String = row
                .try_get("", "rail")
                .map_err(|e| PaymentError::Transport(e.to_string()))?;
            let provider_ref: String = row
                .try_get("", "provider_ref")
                .map_err(|e| PaymentError::Transport(e.to_string()))?;
            let idempotency_key: uuid::Uuid = row
                .try_get("", "idempotency_key")
                .map_err(|e| PaymentError::Transport(e.to_string()))?;
            let status_str: String = row
                .try_get("", "status")
                .map_err(|e| PaymentError::Transport(e.to_string()))?;
            let created_at: chrono::DateTime<chrono::Utc> = row
                .try_get("", "created_at")
                .map_err(|e| PaymentError::Transport(e.to_string()))?;

            let rail = rail_from_str(&rail_str)?;
            let status = status_from_str(&status_str)?;

            let kind = match status {
                PaymentStatus::Pending => PaymentEventKind::Initiated,
                other => PaymentEventKind::Settled {
                    status: other,
                    fees: None,
                    settled_at: None,
                },
            };

            events.push(PaymentEvent {
                rail,
                provider_ref,
                idempotency_key,
                kind,
                recorded_at: created_at,
            });
        }

        Ok(events)
    }
}

impl PostgresEventStore {
    /// Internal helper: SELECT one row by the three-column identity and
    /// synthesize a `PaymentEvent`.
    async fn fetch_by_idempotency_key_and_rail(
        &self,
        key: uuid::Uuid,
        rail: Rail,
        provider_ref: &str,
    ) -> Result<PaymentEvent, PaymentError> {
        let sql = "
            SELECT rail, provider_ref, idempotency_key, status, created_at
            FROM   bobo_payment_intents
            WHERE  idempotency_key = $1
              AND  rail            = $2
              AND  provider_ref    = $3
            LIMIT 1
        ";
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                vec![
                    Value::Uuid(Some(Box::new(key))),
                    Value::String(Some(Box::new(rail_to_str(rail).to_owned()))),
                    Value::String(Some(Box::new(provider_ref.to_owned()))),
                ],
            ))
            .await
            .map_err(|e| PaymentError::Transport(e.to_string()))?
            .ok_or_else(|| {
                PaymentError::Transport(format!("row not found after INSERT for key {key}"))
            })?;

        let rail_str: String = row
            .try_get("", "rail")
            .map_err(|e| PaymentError::Transport(e.to_string()))?;
        let prov_ref: String = row
            .try_get("", "provider_ref")
            .map_err(|e| PaymentError::Transport(e.to_string()))?;
        let stored_key: uuid::Uuid = row
            .try_get("", "idempotency_key")
            .map_err(|e| PaymentError::Transport(e.to_string()))?;
        let status_str: String = row
            .try_get("", "status")
            .map_err(|e| PaymentError::Transport(e.to_string()))?;
        let created_at: chrono::DateTime<chrono::Utc> = row
            .try_get("", "created_at")
            .map_err(|e| PaymentError::Transport(e.to_string()))?;

        let stored_rail = rail_from_str(&rail_str)?;
        let status = status_from_str(&status_str)?;

        let kind = match status {
            PaymentStatus::Pending => PaymentEventKind::Initiated,
            other => PaymentEventKind::Settled {
                status: other,
                fees: None,
                settled_at: None,
            },
        };

        Ok(PaymentEvent {
            rail: stored_rail,
            provider_ref: prov_ref,
            idempotency_key: stored_key,
            kind,
            recorded_at: created_at,
        })
    }
}
