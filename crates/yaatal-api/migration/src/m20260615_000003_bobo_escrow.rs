//! `bobo_escrow` — escrow state per order.
//!
//! One row per order; transitions are performed via
//! `UPDATE ... WHERE state = $expected RETURNING *` (optimistic locking).
//! The five escrow states mirror `EscrowState` in `yaatal-core::commerce::escrow`.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260615_000003_bobo_escrow"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_escrow (
    order_id    BIGINT  PRIMARY KEY REFERENCES bobo_orders(id) ON DELETE RESTRICT,
    state       TEXT    NOT NULL CHECK (state IN (
                            'held',
                            'released',
                            'settled',
                            'disputed',
                            'refunded'
                        )),
    held_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    released_at TIMESTAMPTZ  NULL,
    settled_at  TIMESTAMPTZ  NULL,
    dispute_id  BIGINT       NULL
);
"#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS bobo_escrow CASCADE;")
            .await
            .map(|_| ())
    }
}
