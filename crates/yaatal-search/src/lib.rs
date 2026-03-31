//! Yaatal Search — ColBERT-based semantic search.
//!
//! This crate currently provides a zero-shot evaluation scaffold for retrieval
//! backends. Plug a ColBERT backend into the [`Retriever`] trait, then run
//! [`evaluate_zero_shot`] to compute baseline metrics.

pub mod python_sidecar;
pub mod zero_shot;

pub use python_sidecar::{ColbertHttpRetriever, SidecarIndexDocument};
pub use zero_shot::{
    evaluate_zero_shot, RankedHit, Retriever, SearchDocument, SearchQuery, ZeroShotDataset,
    ZeroShotError, ZeroShotMetrics,
};
