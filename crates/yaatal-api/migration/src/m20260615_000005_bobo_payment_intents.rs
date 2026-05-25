//! `bobo_payment_intents` — bridge between BOBO orders and `yaatal-payments`.
//!
//! The `(rail, provider_ref, idempotency_key)` triple matches
//! `yaatal_payments::events::EventIdentity` — this table is the Postgres
//! backend for `PostgresEventStore` in `yaatal-core::commerce::payment_events_postgres`.
//!
//! `idempotency_key` is UNIQUE on its own (fast lookup by key);
//! the composite `(rail, provider_ref, idempotency_key)` is also UNIQUE
//! to mirror the in-memory store's identity invariant.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260615_000005_bobo_payment_intents"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_payment_intents (
    id               BIGSERIAL   PRIMARY KEY,
    order_id         BIGINT      NOT NULL REFERENCES bobo_orders(id) ON DELETE RESTRICT,
    rail             TEXT        NOT NULL,
    provider_ref     TEXT        NOT NULL,
    idempotency_key  UUID        NOT NULL,
    status           TEXT        NOT NULL CHECK (status IN (
                                     'pending',
                                     'succeeded',
                                     'failed',
                                     'reversed'
                                 )),
    amount_xof       BIGINT      NOT NULL CHECK (amount_xof > 0),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Single-key dedup (fast idempotency lookup path).
    CONSTRAINT uq_bobo_payment_intents_idempotency_key
        UNIQUE (idempotency_key),

    -- Full triple dedup — mirrors EventIdentity from yaatal-payments::events.
    CONSTRAINT uq_bobo_payment_intents_triple
        UNIQUE (rail, provider_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_bobo_payment_intents_order
    ON bobo_payment_intents (order_id);

CREATE INDEX IF NOT EXISTS idx_bobo_payment_intents_status_updated
    ON bobo_payment_intents (status, updated_at);
"#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS bobo_payment_intents CASCADE;")
            .await
            .map(|_| ())
    }
}
