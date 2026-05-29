use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum OrderStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "confirmed")]
    Confirmed,
    #[sea_orm(string_value = "shipped")]
    Shipped,
    #[sea_orm(string_value = "delivered")]
    Delivered,
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PaymentStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "paid")]
    Paid,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "orders")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub buyer_id: String,
    pub seller_id: String,
    pub status: OrderStatus,
    pub payment_method: String,
    pub payment_status: PaymentStatus,
    pub delivery_method: String,
    pub total_cents: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::BuyerId",
        to = "super::profile::Column::Id"
    )]
    Buyer,
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::SellerId",
        to = "super::profile::Column::Id"
    )]
    Seller,
    #[sea_orm(has_many = "super::order_item::Entity")]
    Items,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Buyer.def()
    }
}

impl Related<super::order_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Items.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
