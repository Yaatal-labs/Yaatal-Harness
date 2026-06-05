//! Remediate `profiles.user_id` column type on already-deployed Postgres
//! databases: the original `m20260222_000001` migration created it as native
//! `uuid`, but the sea-orm model types it as `Option<String>` and the auth /
//! profile code binds the Loco user pid as a string. Postgres enforces types
//! strictly, so registration (`INSERT ... user_id = $text`) failed with SQLSTATE
//! 42804 ("column is of type uuid but expression is of type text") and product
//! creation (`WHERE user_id = $text`) failed with 42883 ("operator does not
//! exist: uuid = text"). SQLite (the test backend) has no strict uuid type, so
//! the bug was invisible in tests — classic SQLite-vs-Postgres harness drift.
//!
//! This converts the live column to `text`. It is idempotent (only alters when
//! the column is still `uuid`) and a no-op on SQLite, which already stores the
//! column with text affinity.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260616_000000_fix_profiles_user_id_type"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

        // SQLite stores this column with text affinity already; nothing to do.
        if manager.get_database_backend() == DatabaseBackend::Sqlite {
            return Ok(());
        }

        let conn = manager.get_connection();

        // Only rewrite when the column is still the native `uuid` type, so fresh
        // deploys (where the corrected create-migration already makes it text)
        // and re-runs are no-ops.
        let is_uuid = conn
            .query_one(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT 1 FROM information_schema.columns \
                 WHERE table_name = 'profiles' \
                   AND column_name = 'user_id' \
                   AND data_type = 'uuid'"
                    .to_owned(),
            ))
            .await?
            .is_some();

        if is_uuid {
            // The unique index idx_profiles_user_id is rebuilt automatically by
            // Postgres as part of the column type change.
            conn.execute_unprepared(
                "ALTER TABLE profiles \
                 ALTER COLUMN user_id TYPE text USING user_id::text;",
            )
            .await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Widening uuid -> text is not safely reversible (arbitrary text may not
        // be a valid uuid), so down is intentionally a no-op.
        Ok(())
    }
}
