//! `bobo_orders` — core BOBO commerce order table.
//!
//! Uses raw SQL for the explicit CHECK constraints and DDL. All statements are
//! idempotent. Delivery coordinates are stored as plain lat/lng columns so the
//! engine boots on a stock Postgres (no PostGIS dependency); a geo index can be
//! re-introduced behind PostGIS when geo-search is actually built.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260615_000001_bobo_orders"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Skip on SQLite (in-memory test database). Uses Postgres-specific types.
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Sqlite {
            return Ok(());
        }
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_orders (
    id            BIGSERIAL PRIMARY KEY,
    merchant_id   TEXT      NOT NULL,
    buyer_pid     UUID      NOT NULL,
    total_xof     BIGINT    NOT NULL CHECK (total_xof > 0),
    currency      TEXT      NOT NULL DEFAULT 'XOF',
    state         TEXT      NOT NULL CHECK (state IN (
                                'created',
                                'payment_held',
                                'delivery_confirmed',
                                'released',
                                'settled',
                                'cancelled',
                                'disputed'
                            )),
    delivery_lat  DOUBLE PRECISION  NULL,
    delivery_lng  DOUBLE PRECISION  NULL,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_bobo_orders_merchant_state
    ON bobo_orders (merchant_id, state);

CREATE INDEX IF NOT EXISTS idx_bobo_orders_buyer_created
    ON bobo_orders (buyer_pid, created_at DESC);
"#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS bobo_orders CASCADE;")
            .await
            .map(|_| ())
    }
}
