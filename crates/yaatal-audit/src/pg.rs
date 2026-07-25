//! Postgres `AuditStore` — the documented upgrade from `JsonlAuditStore` (see the
//! `ponytail:` note on that type). This is the *lean* upgrade: it reuses a Postgres the
//! deployment already runs (the Engine hosts one), adds **no new service**, and only
//! changes the medium — flat file → a queryable, concurrent-writer-safe table.
//!
//! **Storage shape.** Each `AuditEvent` is stored as a single `JSONB` `payload`, with
//! `event_id`, `run_id`, and `started_at` mirrored into typed, indexed columns for the two
//! query paths (`by_run`, `in_range`). Storing the whole event as JSONB (rather than a
//! column per field) reuses the crate's existing serde round-trip — already covered by
//! `event_serialization_round_trips` — and survives `AuditEvent` gaining fields without a
//! migration. It does **not** weaken the "digests, not payloads" invariant: an
//! `AuditEvent` already holds only digests of input/output, never the raw text, so the
//! JSONB row is not a second copy of user data.
//!
//! **Autonomy stays L0.** This only records what already happened; it never gates, scores,
//! or mutates config (same contract as the other stores).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use crate::{AuditError, AuditEvent, AuditStore};

/// Idempotent schema: safe to run on every construction (`IF NOT EXISTS`). The two
/// indexes back `by_run` (run_id) and `in_range` (started_at).
const SCHEMA_SQL: &str = "\
CREATE TABLE IF NOT EXISTS audit_events (
    event_id   UUID PRIMARY KEY,
    run_id     UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    payload    JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS audit_events_run_id_idx     ON audit_events (run_id);
CREATE INDEX IF NOT EXISTS audit_events_started_at_idx ON audit_events (started_at);
";

/// Durable, queryable `AuditStore` backed by Postgres. Concurrent-writer safe via the
/// connection pool — the JSONL store's single-writer lock is not needed here.
pub struct PgAuditStore {
    pool: PgPool,
}

impl PgAuditStore {
    /// Connect (pooled) to `database_url` and ensure the table + indexes exist.
    pub async fn connect(database_url: &str) -> Result<Self, AuditError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    /// Build from an existing pool (e.g. sharing the host application's pool) and ensure
    /// the schema. Lets the runner reuse one pool instead of opening a second.
    pub async fn from_pool(pool: PgPool) -> Result<Self, AuditError> {
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<(), AuditError> {
        sqlx::raw_sql(SCHEMA_SQL).execute(&self.pool).await?;
        Ok(())
    }

    /// Deserialize a `payload` column back into an `AuditEvent`. Kept as one place so the
    /// two read paths share it.
    fn from_payload(payload: serde_json::Value) -> Result<AuditEvent, AuditError> {
        Ok(serde_json::from_value(payload)?)
    }
}

#[async_trait]
impl AuditStore for PgAuditStore {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        let payload = serde_json::to_value(&event)?;
        // ON CONFLICT DO NOTHING: append-only + idempotent for at-least-once callers
        // (re-delivering the same event_id is a no-op, not a duplicate row).
        sqlx::query(
            "INSERT INTO audit_events (event_id, run_id, started_at, payload) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event.event_id)
        .bind(event.run_id)
        .bind(event.started_at)
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn by_run(&self, run_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        let payloads = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM audit_events WHERE run_id = $1 ORDER BY started_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        payloads.into_iter().map(Self::from_payload).collect()
    }

    async fn in_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        let payloads = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM audit_events \
             WHERE started_at >= $1 AND started_at <= $2 ORDER BY started_at",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        payloads.into_iter().map(Self::from_payload).collect()
    }

    async fn count(&self) -> Result<usize, AuditError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
            .fetch_one(&self.pool)
            .await?;
        Ok(usize::try_from(n).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionKind, AuditEventBuilder};

    /// No-DB check for the only PG-specific mapping: the JSONB payload round-trips *and*
    /// the mirrored index columns come straight off the event. If this holds, the SQL
    /// (which just binds these values) is the whole story. Runnable with
    /// `cargo test -p yaatal-audit --features postgres`.
    #[test]
    fn payload_round_trips_and_index_columns_match() {
        let event =
            AuditEventBuilder::new(Uuid::new_v4(), "engine:test", ActionKind::ToolCall, "shell")
                .finish("in", "out", true);

        let payload = serde_json::to_value(&event).expect("serialize");
        let back = PgAuditStore::from_payload(payload.clone()).expect("deserialize");

        assert_eq!(back, event);
        // The three mirrored columns are exactly the event's own fields.
        assert_eq!(payload["event_id"], serde_json::json!(event.event_id));
        assert_eq!(payload["run_id"], serde_json::json!(event.run_id));
        assert_eq!(back.started_at, event.started_at);
    }

    /// Full round-trip against a live Postgres. Ignored by default; run with
    /// `AUDIT_PG_URL=postgres://… cargo test -p yaatal-audit --features postgres -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a live Postgres via AUDIT_PG_URL"]
    async fn pg_store_round_trips() {
        let url = std::env::var("AUDIT_PG_URL").expect("AUDIT_PG_URL set");
        let store = PgAuditStore::connect(&url).await.expect("connect");
        let run = Uuid::new_v4();

        store
            .append(
                AuditEventBuilder::new(run, "engine:it", ActionKind::ToolCall, "shell")
                    .finish("i", "o", true),
            )
            .await
            .expect("append");

        let events = store.by_run(run).await.expect("by_run");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action_name, "shell");
    }
}
