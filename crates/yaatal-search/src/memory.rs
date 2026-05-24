use crate::{
    contracts::{SearchFilters, SearchRecord, UpsertDocument},
    errors::SearchError,
    traits::{DocumentStore, Embedder, IndexedPoint, IndexedResult, VectorIndex},
};
use async_trait::async_trait;
use serde_json::Value;
use std::{
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};
use tokio::sync::Mutex;

const DIMENSIONS: usize = 64;

#[derive(Debug, Default, Clone)]
pub struct MemoryEmbedder;

#[async_trait]
impl Embedder for MemoryEmbedder {
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, SearchError> {
        Ok(embed_text(text))
    }

    async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, SearchError> {
        Ok(texts.iter().map(|text| embed_text(text)).collect())
    }
}

#[derive(Debug, Default, Clone)]
pub struct MemoryVectorIndex {
    inner: Arc<Mutex<HashMap<String, IndexedPoint>>>,
}

#[async_trait]
impl VectorIndex for MemoryVectorIndex {
    async fn reset(&self) -> Result<(), SearchError> {
        self.inner.lock().await.clear();
        Ok(())
    }

    async fn upsert(&self, points: Vec<IndexedPoint>) -> Result<usize, SearchError> {
        let mut guard = self.inner.lock().await;
        let indexed = points.len();
        for point in points {
            guard.insert(point.id.clone(), point);
        }
        Ok(indexed)
    }

    async fn search(
        &self,
        vector: &[f32],
        top_k: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<IndexedResult>, SearchError> {
        let guard = self.inner.lock().await;
        let mut scored: Vec<IndexedResult> = guard
            .values()
            .filter(|point| matches_filters(point, filters))
            .map(|point| IndexedResult {
                id: point.id.clone(),
                score: cosine_similarity(vector, &point.vector) as f32,
                record: None,
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        scored.truncate(top_k);
        Ok(scored)
    }
}

#[derive(Debug, Default, Clone)]
pub struct MemoryDocumentStore {
    inner: Arc<Mutex<HashMap<String, SearchRecord>>>,
}

#[async_trait]
impl DocumentStore for MemoryDocumentStore {
    async fn reset(&self) -> Result<(), SearchError> {
        self.inner.lock().await.clear();
        Ok(())
    }

    async fn upsert_documents(&self, documents: Vec<SearchRecord>) -> Result<usize, SearchError> {
        let mut guard = self.inner.lock().await;
        let indexed = documents.len();
        for doc in documents {
            guard.insert(doc.id.clone(), doc);
        }
        Ok(indexed)
    }

    async fn fetch_documents(&self, ids: &[String]) -> Result<Vec<SearchRecord>, SearchError> {
        let guard = self.inner.lock().await;
        Ok(ids.iter().filter_map(|id| guard.get(id).cloned()).collect())
    }
}

fn embed_text(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; DIMENSIONS];
    for token in tokenize(text) {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % DIMENSIONS;
        vector[idx] += 1.0;
    }

    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }

    vector
}

fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    let len = left.len().min(right.len());
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for idx in 0..len {
        let l = left[idx] as f64;
        let r = right[idx] as f64;
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn matches_filters(point: &IndexedPoint, filters: &SearchFilters) -> bool {
    if let Some(source) = &filters.source {
        if point.source != *source {
            return false;
        }
    }

    if let Some(lang) = filters
        .metadata
        .get("lang")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    {
        match point.lang.as_deref() {
            Some(value) if value == lang => {}
            _ => return false,
        }
    }

    if let Some(market) = filters
        .metadata
        .get("market")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    {
        match point.market.as_deref() {
            Some(value) if value == market => {}
            _ => return false,
        }
    }

    for (key, value) in &filters.metadata {
        match key.as_str() {
            "lang" | "market" => continue,
            "source_id" => {
                if point.source_id != value.as_str().unwrap_or_default() {
                    return false;
                }
            }
            "title" => {
                if point.title.as_deref() != value.as_str() {
                    return false;
                }
            }
            _ => {
                let Some(point_value) = point.metadata.get(key) else {
                    return false;
                };

                if point_value != value {
                    return false;
                }
            }
        }
    }

    true
}

impl From<&UpsertDocument> for SearchRecord {
    fn from(value: &UpsertDocument) -> Self {
        Self {
            id: value.id.clone(),
            source: value.source.clone(),
            source_id: value.source_id.clone(),
            title: value.title.clone(),
            text: value.text.clone(),
            lang: value.lang.clone(),
            market: value.market.clone(),
            metadata: value.metadata.clone(),
        }
    }
}

impl From<&SearchRecord> for IndexedPoint {
    fn from(value: &SearchRecord) -> Self {
        Self {
            id: value.id.clone(),
            vector: embed_text(&value.text),
            source: value.source.clone(),
            source_id: value.source_id.clone(),
            title: value.title.clone(),
            text: value.text.clone(),
            lang: value.lang.clone(),
            market: value.market.clone(),
            metadata: value.metadata.clone(),
        }
    }
}
