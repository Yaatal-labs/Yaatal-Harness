//! `bobo_ledger` — append-only financial ledger for BOBO orders.
//!
//! A trigger on UPDATE and DELETE raises an exception to enforce immutability
//! at the database level. Mirrors the immutable-log property of
//! `yaatal-payments::payment_events`.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260615_000002_bobo_ledger"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Sqlite {
            return Ok(());
        }
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_ledger (
    id          BIGSERIAL   PRIMARY KEY,
    order_id    BIGINT      NOT NULL REFERENCES bobo_orders(id) ON DELETE RESTRICT,
    direction   TEXT        NOT NULL CHECK (direction IN ('credit', 'debit')),
    amount_xof  BIGINT      NOT NULL CHECK (amount_xof > 0),
    kind        TEXT        NOT NULL,
    ref_id      TEXT        NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_bobo_ledger_order_created
    ON bobo_ledger (order_id, created_at);

-- Append-only enforcement: block UPDATE and DELETE at the DB level.
CREATE OR REPLACE FUNCTION bobo_ledger_append_only()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'bobo_ledger is append-only';
END;
$$;

DROP TRIGGER IF EXISTS trg_bobo_ledger_no_update ON bobo_ledger;
CREATE TRIGGER trg_bobo_ledger_no_update
    BEFORE UPDATE ON bobo_ledger
    FOR EACH ROW EXECUTE FUNCTION bobo_ledger_append_only();

DROP TRIGGER IF EXISTS trg_bobo_ledger_no_delete ON bobo_ledger;
CREATE TRIGGER trg_bobo_ledger_no_delete
    BEFORE DELETE ON bobo_ledger
    FOR EACH ROW EXECUTE FUNCTION bobo_ledger_append_only();
"#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
DROP TABLE IF EXISTS bobo_ledger CASCADE;
DROP FUNCTION IF EXISTS bobo_ledger_append_only() CASCADE;
"#,
            )
            .await
            .map(|_| ())
    }
}
