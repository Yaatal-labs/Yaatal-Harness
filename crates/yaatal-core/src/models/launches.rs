use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "launches")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub author_id: String,
    pub name: String,
    pub tagline: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub logo_url: Option<String>,
    pub category: Option<String>,
    pub upvotes: i32,
    pub launch_date: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::AuthorId",
        to = "super::profile::Column::Id"
    )]
    Author,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Author.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
