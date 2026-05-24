use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PostType {
    #[sea_orm(string_value = "question")]
    Question,
    #[sea_orm(string_value = "tutorial")]
    Tutorial,
    #[sea_orm(string_value = "discussion")]
    Discussion,
    #[sea_orm(string_value = "showcase")]
    Showcase,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub author_id: String,
    pub title: String,
    pub content: String,
    pub r#type: PostType,
    pub category: Option<String>,
    pub tags: Option<String>,
    pub upvotes: i32,
    #[sea_orm(default_value = 0)]
    pub comment_count: i32,
    #[sea_orm(default_value = 0)]
    pub is_pinned: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::AuthorId",
        to = "super::profile::Column::Id"
    )]
    Author,
    #[sea_orm(has_many = "super::comments::Entity")]
    Comments,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Author.def()
    }
}

impl Related<super::comments::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Comments.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
