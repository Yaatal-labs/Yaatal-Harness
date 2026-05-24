use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchRecord {
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub title: Option<String>,
    pub text: String,
    pub lang: Option<String>,
    pub market: Option<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpsertDocument {
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub title: Option<String>,
    pub text: String,
    pub lang: Option<String>,
    pub market: Option<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SearchFilters {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub market: Option<String>,
    #[serde(default)]
    pub filters: Option<SearchFilters>,
}

fn default_top_k() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub text: String,
    pub score: f32,
    pub source: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexUpsertRequest {
    pub documents: Vec<UpsertDocument>,
    #[serde(default)]
    pub reset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexUpsertResponse {
    pub indexed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub status: &'static str,
}
