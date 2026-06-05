//! Add `user_id` UUID column to profiles — links Loco `users` to yaatal-core `profiles`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add user_id column (nullable for backward compat with existing rows).
        // Stored as text, not native `uuid`: the sea-orm model types this column
        // as `Option<String>` and the auth/profile code binds the Loco user pid as
        // a string. On SQLite (test backend) `uuid` has no strict type so a string
        // binds fine, but Postgres rejects text→uuid inserts and `uuid = text`
        // filters (SQLSTATE 42804 / 42883). Keep it text to match the model and the
        // rest of the schema, whose id columns are all `.string()`.
        manager
            .alter_table(
                Table::alter()
                    .table(Profiles::Table)
                    .add_column(ColumnDef::new(Profiles::UserId).string().null())
                    .to_owned(),
            )
            .await?;

        // Index for fast lookup by user_id
        manager
            .create_index(
                Index::create()
                    .name("idx_profiles_user_id")
                    .table(Profiles::Table)
                    .col(Profiles::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Profiles::Table)
                    .drop_column(Profiles::UserId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Profiles {
    Table,
    UserId,
}
