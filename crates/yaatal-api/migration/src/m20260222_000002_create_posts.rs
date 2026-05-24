//! Create `posts` table in Loco's dev SQLite — mirrors `001_initial.sql` schema.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Posts::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Posts::AuthorId).string().not_null())
                    .col(ColumnDef::new(Posts::Title).string().not_null())
                    .col(ColumnDef::new(Posts::Content).string().not_null())
                    .col(
                        ColumnDef::new(Posts::Type)
                            .string()
                            .not_null()
                            .default("discussion"),
                    )
                    .col(ColumnDef::new(Posts::Category).string().null())
                    .col(ColumnDef::new(Posts::Tags).string().null())
                    .col(
                        ColumnDef::new(Posts::Upvotes)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Posts::CommentCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Posts::IsPinned)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Posts::CreatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .col(ColumnDef::new(Posts::UpdatedAt).string().null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Posts::Table, Posts::AuthorId)
                            .to(Profiles::Table, Profiles::Id),
                    )
                    .to_owned(),
            )
            .await?;

        // Performance indexes matching 001_initial.sql
        manager
            .create_index(
                Index::create()
                    .name("idx_posts_author")
                    .table(Posts::Table)
                    .col(Posts::AuthorId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_posts_type")
                    .table(Posts::Table)
                    .col(Posts::Type)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_posts_created")
                    .table(Posts::Table)
                    .col(Posts::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Posts::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Posts {
    Table,
    Id,
    AuthorId,
    Title,
    Content,
    Type,
    Category,
    Tags,
    Upvotes,
    CommentCount,
    IsPinned,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Profiles {
    Table,
    Id,
}
