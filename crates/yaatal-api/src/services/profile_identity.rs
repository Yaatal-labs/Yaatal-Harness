//! Helpers for resolving a domain profile from an authenticated user PID.

use crate::models::users;
use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
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

/// Resolve the profile model linked to a Loco user PID, backfilling one for
/// legacy users that predate linked profile creation.
pub async fn ensure_profile_for_user_pid(
    db: &sea_orm::DatabaseConnection,
    user_pid: &str,
) -> Result<profile::Model> {
    if let Ok(profile) = resolve_profile_for_user_pid(db, user_pid).await {
        return Ok(profile);
    }

    let user = users::Model::find_by_pid(db, user_pid)
        .await
        .map_err(|error| {
            Error::string(&format!(
                "failed to resolve user for profile backfill: {error}"
            ))
        })?;

    let now = chrono::Utc::now().to_rfc3339();
    let profile_id = uuid::Uuid::new_v4().to_string();
    let profile_model = profile::ActiveModel {
        id: Set(profile_id),
        user_id: Set(Some(user.pid.to_string())),
        username: Set(None),
        display_name: Set(Some(user.name.clone())),
        bio: Set(None),
        avatar_url: Set(None),
        xp: Set(0),
        level: Set(1),
        streak_days: Set(0),
        last_active_at: Set(None),
        interests: Set(None),
        onboarding_complete: Set(0),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    };

    match profile::Entity::insert(profile_model).exec(db).await {
        Ok(_) => {}
        Err(error) => {
            if let Ok(profile) = resolve_profile_for_user_pid(db, user_pid).await {
                return Ok(profile);
            }

            return Err(Error::string(&format!(
                "failed to backfill linked profile: {error}"
            )));
        }
    }

    resolve_profile_for_user_pid(db, user_pid).await
}

/// Resolve the domain profile id linked to a Loco user PID, creating a linked
/// profile for legacy users when needed.
pub async fn ensure_profile_id_for_user_pid(
    db: &sea_orm::DatabaseConnection,
    user_pid: &str,
) -> Result<String> {
    ensure_profile_for_user_pid(db, user_pid)
        .await
        .map(|profile| profile.id)
}
