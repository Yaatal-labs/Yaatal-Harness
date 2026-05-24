use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Links to Loco `users.pid` (UUID). Nullable for backward compat.
    #[sea_orm(unique)]
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub xp: i32,
    pub level: i32,
    pub streak_days: i32,
    pub last_active_at: Option<String>,
    pub interests: Option<String>,
    #[sea_orm(default_value = 0)]
    pub onboarding_complete: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::post::Entity")]
    Posts,
    #[sea_orm(has_many = "super::comments::Entity")]
    Comments,
    #[sea_orm(has_many = "super::upvotes::Entity")]
    Upvotes,
    #[sea_orm(has_many = "super::launches::Entity")]
    Launches,
    #[sea_orm(has_many = "super::achievements::Entity")]
    Achievements,
    #[sea_orm(has_many = "super::bo_conversations::Entity")]
    BoConversations,
    #[sea_orm(has_many = "super::bookmarks::Entity")]
    Bookmarks,
    #[sea_orm(has_many = "super::user_security_keys::Entity")]
    UserSecurityKeys,
}

impl Related<super::post::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Posts.def()
    }
}

impl Related<super::comments::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Comments.def()
    }
}

impl Related<super::upvotes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Upvotes.def()
    }
}

impl Related<super::launches::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Launches.def()
    }
}

impl Related<super::achievements::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Achievements.def()
    }
}

impl Related<super::bo_conversations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BoConversations.def()
    }
}

impl Related<super::bookmarks::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Bookmarks.def()
    }
}

impl Related<super::user_security_keys::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserSecurityKeys.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
