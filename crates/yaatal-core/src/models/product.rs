use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price_cents: i32,
    pub discount_price_cents: Option<i32>,
    pub stock: i32,
    pub category: String,
    pub images: Option<String>,
    #[sea_orm(default_value = 1)]
    pub is_active: i32,
    #[sea_orm(default_value = 0)]
    pub upvotes: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::MerchantId",
        to = "super::profile::Column::Id"
    )]
    Merchant,
    #[sea_orm(has_many = "super::order_item::Entity")]
    OrderItems,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Merchant.def()
    }
}

impl Related<super::order_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrderItems.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
