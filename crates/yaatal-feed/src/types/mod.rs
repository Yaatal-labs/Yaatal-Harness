//! Public type surface for the feed crate.
//!
//! Ranking types live in `feed`; ingestion support types live in `ingestion`.

pub mod feed;
pub mod ingestion;

pub use feed::*;
pub use ingestion::*;
