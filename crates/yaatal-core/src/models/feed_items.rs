use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "feed_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub source: String,
    pub title: String,
    pub url: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub category: Option<String>,
    pub published_at: Option<String>,
    pub ingested_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
// feed_items has no foreign keys in 001_initial.sql.
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
