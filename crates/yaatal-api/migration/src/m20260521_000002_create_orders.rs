use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Orders::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Orders::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Orders::BuyerId).string().not_null())
                    .col(ColumnDef::new(Orders::SellerId).string().not_null())
                    .col(
                        ColumnDef::new(Orders::Status)
                            .string()
                            .not_null()
                            .default("pending"),
                    )
                    .col(ColumnDef::new(Orders::PaymentMethod).string().not_null())
                    .col(
                        ColumnDef::new(Orders::PaymentStatus)
                            .string()
                            .not_null()
                            .default("pending"),
                    )
                    .col(ColumnDef::new(Orders::DeliveryMethod).string().not_null())
                    .col(ColumnDef::new(Orders::TotalCents).integer().not_null())
                    .col(
                        ColumnDef::new(Orders::CreatedAt)
                            .string()
                            .not_null()
                            .default("datetime('now')"),
                    )
                    .col(ColumnDef::new(Orders::UpdatedAt).string().null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Orders::Table, Orders::BuyerId)
                            .to(Profiles::Table, Profiles::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Orders::Table, Orders::SellerId)
                            .to(Profiles::Table, Profiles::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_buyer")
                    .table(Orders::Table)
                    .col(Orders::BuyerId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_seller")
                    .table(Orders::Table)
                    .col(Orders::SellerId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_status")
                    .table(Orders::Table)
                    .col(Orders::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_created")
                    .table(Orders::Table)
                    .col(Orders::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Orders::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Orders {
    Table,
    Id,
    BuyerId,
    SellerId,
    Status,
    PaymentMethod,
    PaymentStatus,
    DeliveryMethod,
    TotalCents,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Profiles {
    Table,
    Id,
}
