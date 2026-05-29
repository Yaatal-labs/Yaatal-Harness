#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
mod m20220101_000001_users;
mod m20260222_000000_create_profiles;
mod m20260222_000001_add_user_id_to_profiles;
mod m20260222_000002_create_posts;
mod m20260222_000003_create_comments;
mod m20260521_000001_create_products;
mod m20260521_000002_create_orders;
mod m20260521_000003_create_order_items;
mod m20260601_000000_extensions;
mod m20260615_000001_bobo_orders;
mod m20260615_000002_bobo_ledger;
mod m20260615_000003_bobo_escrow;
mod m20260615_000004_bobo_kyc;
mod m20260615_000005_bobo_payment_intents;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            // Extensions MUST run first — PostGIS geography type is needed
            // by bobo_orders.delivery_location below.
            Box::new(m20260601_000000_extensions::Migration),
            // yokk-engine baseline schema.
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20260222_000000_create_profiles::Migration),
            Box::new(m20260222_000001_add_user_id_to_profiles::Migration),
            Box::new(m20260222_000002_create_posts::Migration),
            Box::new(m20260222_000003_create_comments::Migration),
            Box::new(m20260521_000001_create_products::Migration),
            Box::new(m20260521_000002_create_orders::Migration),
            Box::new(m20260521_000003_create_order_items::Migration),
            // BOBO commerce schema (Lane 5b).
            Box::new(m20260615_000001_bobo_orders::Migration),
            Box::new(m20260615_000002_bobo_ledger::Migration),
            Box::new(m20260615_000003_bobo_escrow::Migration),
            Box::new(m20260615_000004_bobo_kyc::Migration),
            Box::new(m20260615_000005_bobo_payment_intents::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
