//! Create `profiles` table for domain identity rows used by posts/comments/xp.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Profiles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Profiles::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Profiles::Username)
                            .string()
                            .null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Profiles::DisplayName).string().null())
                    .col(ColumnDef::new(Profiles::Bio).string().null())
                    .col(ColumnDef::new(Profiles::AvatarUrl).string().null())
                    .col(ColumnDef::new(Profiles::Xp).integer().not_null().default(0))
                    .col(
                        ColumnDef::new(Profiles::Level)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Profiles::StreakDays)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Profiles::LastActiveAt).string().null())
                    .col(ColumnDef::new(Profiles::Interests).string().null())
                    .col(
                        ColumnDef::new(Profiles::OnboardingComplete)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Profiles::CreatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .col(
                        ColumnDef::new(Profiles::UpdatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Profiles::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Profiles {
    Table,
    Id,
    Username,
    DisplayName,
    Bio,
    AvatarUrl,
    Xp,
    Level,
    StreakDays,
    LastActiveAt,
    Interests,
    OnboardingComplete,
    CreatedAt,
    UpdatedAt,
}
