//! Helpers for resolving a domain profile from an authenticated user PID.

use loco_rs::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use yaatal_core::models::profile;

/// Resolve the profile model linked to a Loco user PID.
pub async fn resolve_profile_for_user_pid(
    db: &sea_orm::DatabaseConnection,
    user_pid: &str,
) -> Result<profile::Model> {
    profile::Entity::find()
        .filter(profile::Column::UserId.eq(user_pid.to_string()))
        .one(db)
        .await?
        .ok_or(Error::NotFound)
}

/// Resolve the domain profile id linked to a Loco user PID.
pub async fn resolve_profile_id_for_user_pid(
    db: &sea_orm::DatabaseConnection,
    user_pid: &str,
) -> Result<String> {
    resolve_profile_for_user_pid(db, user_pid)
        .await
        .map(|profile| profile.id)
}
