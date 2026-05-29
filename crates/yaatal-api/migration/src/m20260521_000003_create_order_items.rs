use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OrderItems::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OrderItems::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OrderItems::OrderId).string().not_null())
                    .col(ColumnDef::new(OrderItems::ProductId).string().not_null())
                    .col(
                        ColumnDef::new(OrderItems::Quantity)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(OrderItems::UnitPriceCents)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(OrderItems::Table, OrderItems::OrderId)
                            .to(Orders::Table, Orders::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(OrderItems::Table, OrderItems::ProductId)
                            .to(Products::Table, Products::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_order_items_order")
                    .table(OrderItems::Table)
                    .col(OrderItems::OrderId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_order_items_product")
                    .table(OrderItems::Table)
                    .col(OrderItems::ProductId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OrderItems::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum OrderItems {
    Table,
    Id,
    OrderId,
    ProductId,
    Quantity,
    UnitPriceCents,
}

#[derive(Iden)]
enum Orders {
    Table,
    Id,
}

#[derive(Iden)]
enum Products {
    Table,
    Id,
}
