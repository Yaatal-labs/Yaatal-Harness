use sea_orm_migration::prelude::*;

/// Enable Postgres extensions required by Yaatal Engine.
/// All statements are idempotent (IF NOT EXISTS).
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260601_000000_extensions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        // Skip on SQLite (in-memory test database). Extensions are Postgres-only.
        if manager.get_database_backend() == DatabaseBackend::Sqlite {
            return Ok(());
        }
        let conn = manager.get_connection();
        // Best-effort: these extensions are optional / forward-looking. A managed
        // Postgres without them (e.g. Railway's stock postgres-ssl image) must
        // still boot. Probe pg_available_extensions first so a missing extension
        // never errors and poisons this migration's transaction.
        for ext in ["vector", "pg_cron", "postgis", "pg_stat_statements"] {
            let available = conn
                .query_one(Statement::from_string(
                    DatabaseBackend::Postgres,
                    format!("SELECT 1 FROM pg_available_extensions WHERE name = '{ext}'"),
                ))
                .await?
                .is_some();
            if available {
                conn.execute_unprepared(&format!("CREATE EXTENSION IF NOT EXISTS {ext};"))
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Extensions are shared Postgres-level objects; we do not drop them
        // automatically to avoid breaking other databases on the same cluster.
        Ok(())
    }
}
