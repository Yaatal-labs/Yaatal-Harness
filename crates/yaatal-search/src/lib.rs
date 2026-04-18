//! Yaatal Search.
//!
//! This crate now serves two roles:
//! - a runnable search service surface for the Bo-Plex integration path
//! - a legacy zero-shot retrieval/evaluation harness
//!
//! The service path is intentionally pluggable. The default runnable mode is
//! in-memory so the crate can be exercised locally without external services.

pub mod config;
pub mod contracts;
pub mod errors;
pub mod http;
pub mod memory;
pub mod service;
pub mod traits;

pub mod python_sidecar;
pub mod zero_shot;

pub use config::SearchServiceConfig;
pub use contracts::{
    IndexUpsertRequest, IndexUpsertResponse, SearchFilters, SearchHit, SearchRecord, SearchRequest,
    SearchResponse, UpsertDocument,
};
pub use errors::SearchError;
pub use http::{router, run};
pub use memory::{MemoryDocumentStore, MemoryEmbedder, MemoryVectorIndex};
pub use python_sidecar::{ColbertHttpRetriever, SidecarIndexDocument};
pub use service::SearchService;
pub use traits::{
    BgeM3HttpEmbedder, DocumentStore, Embedder, IndexedPoint, IndexedResult, PostgresDocumentStore,
    QdrantHttpIndex, VectorIndex,
};
pub use zero_shot::{
    evaluate_zero_shot, RankedHit, Retriever, SearchDocument, SearchQuery, ZeroShotDataset,
    ZeroShotError, ZeroShotMetrics,
};
