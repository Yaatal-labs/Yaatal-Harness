#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
mod m20220101_000001_users;
mod m20260222_000000_create_profiles;
mod m20260222_000001_add_user_id_to_profiles;
mod m20260222_000002_create_posts;
mod m20260222_000003_create_comments;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20260222_000000_create_profiles::Migration),
            Box::new(m20260222_000001_add_user_id_to_profiles::Migration),
            Box::new(m20260222_000002_create_posts::Migration),
            Box::new(m20260222_000003_create_comments::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
