use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Products::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Products::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Products::MerchantId).string().not_null())
                    .col(ColumnDef::new(Products::Name).string().not_null())
                    .col(ColumnDef::new(Products::Description).string().null())
                    .col(ColumnDef::new(Products::PriceCents).integer().not_null())
                    .col(
                        ColumnDef::new(Products::DiscountPriceCents)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Products::Stock)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Products::Category).string().not_null())
                    .col(ColumnDef::new(Products::Images).string().null())
                    .col(
                        ColumnDef::new(Products::IsActive)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Products::Upvotes)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Products::CreatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .col(ColumnDef::new(Products::UpdatedAt).string().null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Products::Table, Products::MerchantId)
                            .to(Profiles::Table, Profiles::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_merchant")
                    .table(Products::Table)
                    .col(Products::MerchantId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_category")
                    .table(Products::Table)
                    .col(Products::Category)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_active")
                    .table(Products::Table)
                    .col(Products::IsActive)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_created")
                    .table(Products::Table)
                    .col(Products::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Products::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Products {
    Table,
    Id,
    MerchantId,
    Name,
    Description,
    PriceCents,
    DiscountPriceCents,
    Stock,
    Category,
    Images,
    IsActive,
    Upvotes,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Profiles {
    Table,
    Id,
}
