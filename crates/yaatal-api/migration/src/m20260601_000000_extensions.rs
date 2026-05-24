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
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE EXTENSION IF NOT EXISTS vector; \
                 CREATE EXTENSION IF NOT EXISTS pg_cron; \
                 CREATE EXTENSION IF NOT EXISTS postgis; \
                 CREATE EXTENSION IF NOT EXISTS pg_stat_statements;",
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Extensions are shared Postgres-level objects; we do not drop them
        // automatically to avoid breaking other databases on the same cluster.
        Ok(())
    }
}
