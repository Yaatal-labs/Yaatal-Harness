//! `bobo_orders` — core BOBO commerce order table.
//!
//! Uses raw SQL because PostGIS `geography(Point,4326)` cannot be expressed
//! with the sea-orm schema builder. All statements are idempotent.

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
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_orders (
    id            BIGSERIAL PRIMARY KEY,
    merchant_id   BIGINT    NOT NULL,
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
    delivery_location  geography(Point, 4326)  NULL,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_bobo_orders_merchant_state
    ON bobo_orders (merchant_id, state);

CREATE INDEX IF NOT EXISTS idx_bobo_orders_buyer_created
    ON bobo_orders (buyer_pid, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_bobo_orders_delivery_location
    ON bobo_orders USING GIST (delivery_location);
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
