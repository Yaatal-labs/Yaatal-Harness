//! Yaatal Core — shared infrastructure for African-first applications
//!
//! This crate provides:
//! - Database models (Turso/libSQL via SeaORM)
//! - AI cascade router (5-tier, offline-first)
//! - Gamification engine (XP, levels, achievements)
//! - Design tokens (Sunset Over Dakar palette)
//! - Content sanitization and business logic

pub mod ai;
pub mod auth;
pub mod db;
pub mod design;
pub mod gamification;
pub mod models;
pub mod sanitize;

pub use ai::network::{NetworkCondition, NetworkGate};
pub use ai::rate_limit::RateLimiterPool;
pub use ai::router::AiRouter;
pub use db::{connect, connect_from_config_file, load_config_from_file, run_migrations_from_file};
pub use design::tokens;
pub use gamification::xp;
