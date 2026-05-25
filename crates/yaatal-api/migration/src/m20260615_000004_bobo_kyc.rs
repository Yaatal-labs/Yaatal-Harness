//! `bobo_kyc` — KYC profiles for BOBO participants (PII — Sovereign-tagged).
//!
//! `document_hash` stores a SHA-256 digest of the submitted document; the raw
//! document is never persisted here. This table will be annotated
//! `Sensitivity::Sovereign` in Lane 6.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260615_000004_bobo_kyc"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS bobo_kyc (
    pid             UUID  PRIMARY KEY,
    status          TEXT  NOT NULL CHECK (status IN (
                              'unverified',
                              'pending',
                              'verified',
                              'rejected'
                          )),
    provider        TEXT  NOT NULL,
    smile_id_ref    TEXT  NULL,
    verified_at     TIMESTAMPTZ  NULL,
    -- SHA-256 digest of the submitted identity document; raw document never stored.
    document_hash   BYTEA  NULL,
    jurisdiction    TEXT  NOT NULL,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_bobo_kyc_jurisdiction_status
    ON bobo_kyc (jurisdiction, status);
"#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS bobo_kyc CASCADE;")
            .await
            .map(|_| ())
    }
}
