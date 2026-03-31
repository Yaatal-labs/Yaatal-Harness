//! XP integration service — awards gamification points on user actions.
//!
//! Wraps `yaatal_core::gamification::xp` and persists XP increments to the profiles table.

use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, EntityTrait};
use yaatal_core::gamification::xp::XpAction;
use yaatal_core::models::profile;

use crate::services::profile_identity;

/// Award XP for the given action, incrementing the linked profile's `xp` column.
///
/// Returns the new XP total.
///
/// # Errors
///
/// Returns `loco_rs::Error` if the linked profile is not found or the DB update fails.
pub async fn award_xp_by_user_pid(
    db: &sea_orm::DatabaseConnection,
    user_pid: &str,
    action: XpAction,
) -> Result<i32> {
    let profile = profile_identity::resolve_profile_for_user_pid(db, user_pid).await?;

    let points = action.points() as i32;
    let new_xp = profile.xp + points;

    let mut active: profile::ActiveModel = profile.into();
    active.xp = Set(new_xp);
    profile::Entity::update(active).exec(db).await?;

    Ok(new_xp)
}
