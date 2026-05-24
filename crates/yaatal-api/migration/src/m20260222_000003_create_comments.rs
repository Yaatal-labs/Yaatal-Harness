//! Create `comments` table in Loco's dev SQLite — mirrors `001_initial.sql` schema.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Comments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Comments::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Comments::PostId).string().not_null())
                    .col(ColumnDef::new(Comments::AuthorId).string().not_null())
                    .col(ColumnDef::new(Comments::ParentId).string().null())
                    .col(ColumnDef::new(Comments::Content).string().not_null())
                    .col(
                        ColumnDef::new(Comments::IsAccepted)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Comments::Upvotes)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Comments::VoiceUrl).string().null())
                    .col(
                        ColumnDef::new(Comments::CreatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Comments::Table, Comments::PostId)
                            .to(Posts::Table, Posts::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Comments::Table, Comments::AuthorId)
                            .to(Profiles::Table, Profiles::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Comments::Table, Comments::ParentId)
                            .to(Comments::Table, Comments::Id),
                    )
                    .to_owned(),
            )
            .await?;

        // Performance indexes matching 001_initial.sql
        manager
            .create_index(
                Index::create()
                    .name("idx_comments_post")
                    .table(Comments::Table)
                    .col(Comments::PostId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_comments_author")
                    .table(Comments::Table)
                    .col(Comments::AuthorId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Comments::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Comments {
    Table,
    Id,
    PostId,
    AuthorId,
    ParentId,
    Content,
    IsAccepted,
    Upvotes,
    VoiceUrl,
    CreatedAt,
}

#[derive(Iden)]
enum Posts {
    Table,
    Id,
}

#[derive(Iden)]
enum Profiles {
    Table,
    Id,
}
