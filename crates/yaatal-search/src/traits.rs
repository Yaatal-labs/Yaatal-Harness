use crate::{contracts::SearchFilters, contracts::SearchRecord, errors::SearchError};
use async_trait::async_trait;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub source: String,
    pub lang: Option<String>,
    pub market: Option<String>,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedResult {
    pub id: String,
    pub score: f32,
}

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, SearchError>;

    async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, SearchError>;
}

#[async_trait]
pub trait VectorIndex: Send + Sync {
    async fn reset(&self) -> Result<(), SearchError>;

    async fn upsert(&self, points: Vec<IndexedPoint>) -> Result<usize, SearchError>;

    async fn search(
        &self,
        vector: &[f32],
        top_k: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<IndexedResult>, SearchError>;
}

#[async_trait]
pub trait DocumentStore: Send + Sync {
    async fn reset(&self) -> Result<(), SearchError>;

    async fn upsert_documents(&self, documents: Vec<SearchRecord>) -> Result<usize, SearchError>;

    async fn fetch_documents(&self, ids: &[String]) -> Result<Vec<SearchRecord>, SearchError>;
}

#[derive(Debug, Clone)]
pub struct BgeM3HttpEmbedder {
    base_url: String,
}

impl BgeM3HttpEmbedder {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[derive(Debug, Clone)]
pub struct QdrantHttpIndex {
    base_url: String,
}

impl QdrantHttpIndex {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[derive(Debug, Clone)]
pub struct PostgresDocumentStore {
    database_url: String,
}

impl PostgresDocumentStore {
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
        }
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }
}
