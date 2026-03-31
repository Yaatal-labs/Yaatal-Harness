//! SeaORM repository implementations for yaatal-feed pipeline.

pub mod discovery_repository;
pub mod post_repository;

pub use discovery_repository::SeaOrmDiscoveryRepository;
pub use post_repository::SeaOrmPostRepository;
