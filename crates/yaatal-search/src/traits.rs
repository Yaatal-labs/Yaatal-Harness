use crate::{contracts::SearchFilters, contracts::SearchRecord, errors::SearchError};
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub source: String,
    pub source_id: String,
    pub title: Option<String>,
    pub text: String,
    pub lang: Option<String>,
    pub market: Option<String>,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedResult {
    pub id: String,
    pub score: f32,
    pub record: Option<SearchRecord>,
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
    http: reqwest::Client,
}

impl BgeM3HttpEmbedder {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl Embedder for BgeM3HttpEmbedder {
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, SearchError> {
        let payload = serde_json::json!({ "text": text });
        let response = self
            .http
            .post(format!("{}/embed/query", self.base_url))
            .json(&payload)
            .send()
            .await
            .map_err(|error| SearchError::Embedder(error.to_string()))?;

        parse_embedding_response(response).await
    }

    async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, SearchError> {
        let payload = serde_json::json!({ "texts": texts });
        let response = self
            .http
            .post(format!("{}/embed/documents", self.base_url))
            .json(&payload)
            .send()
            .await
            .map_err(|error| SearchError::Embedder(error.to_string()))?;

        parse_embeddings_response(response).await
    }
}

#[derive(Debug, Clone)]
pub struct QdrantHttpIndex {
    base_url: String,
    collection: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl QdrantHttpIndex {
    pub fn new(
        base_url: impl Into<String>,
        collection: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            collection: collection.into(),
            api_key,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn collection(&self) -> &str {
        &self.collection
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.http.request(
            method,
            format!("{}/{}", self.base_url, path.trim_start_matches('/')),
        );

        if let Some(api_key) = &self.api_key {
            request.header("api-key", api_key)
        } else {
            request
        }
    }

    async fn ensure_collection(&self, dimensions: usize) -> Result<(), SearchError> {
        let get_response = self
            .request(
                reqwest::Method::GET,
                &format!("collections/{}", self.collection),
            )
            .send()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;

        if get_response.status().is_success() {
            return Ok(());
        }

        if get_response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = get_response.status();
            let body = get_response.text().await.unwrap_or_default();
            return Err(SearchError::Index(format!(
                "Qdrant collection lookup failed with HTTP {status}: {body}"
            )));
        }

        let create_response = self
            .request(
                reqwest::Method::PUT,
                &format!("collections/{}", self.collection),
            )
            .json(&serde_json::json!({
                "vectors": {
                    "size": dimensions,
                    "distance": "Cosine"
                }
            }))
            .send()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;

        if create_response.status().is_success() {
            Ok(())
        } else {
            let status = create_response.status();
            let body = create_response.text().await.unwrap_or_default();
            Err(SearchError::Index(format!(
                "Qdrant collection creation failed with HTTP {status}: {body}"
            )))
        }
    }
}

#[async_trait]
impl VectorIndex for QdrantHttpIndex {
    async fn reset(&self) -> Result<(), SearchError> {
        let response = self
            .request(
                reqwest::Method::DELETE,
                &format!("collections/{}", self.collection),
            )
            .send()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;

        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(SearchError::Index(format!(
                "Qdrant collection reset failed with HTTP {status}: {body}"
            )))
        }
    }

    async fn upsert(&self, points: Vec<IndexedPoint>) -> Result<usize, SearchError> {
        if points.is_empty() {
            return Ok(0);
        }

        let dimensions = points
            .first()
            .map(|point| point.vector.len())
            .ok_or_else(|| SearchError::Index("missing vector dimensions".to_string()))?;
        self.ensure_collection(dimensions).await?;

        let payload_points: Vec<Value> = points
            .iter()
            .map(|point| {
                serde_json::json!({
                    "id": point.id,
                    "vector": point.vector,
                    "payload": payload_from_point(point),
                })
            })
            .collect();

        let indexed = payload_points.len();
        let response = self
            .request(
                reqwest::Method::PUT,
                &format!("collections/{}/points?wait=true", self.collection),
            )
            .json(&serde_json::json!({ "points": payload_points }))
            .send()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;

        if response.status().is_success() {
            Ok(indexed)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(SearchError::Index(format!(
                "Qdrant point upsert failed with HTTP {status}: {body}"
            )))
        }
    }

    async fn search(
        &self,
        vector: &[f32],
        top_k: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<IndexedResult>, SearchError> {
        let mut body = serde_json::json!({
            "vector": vector,
            "limit": top_k,
            "with_payload": true,
        });

        if let Some(filter) = build_qdrant_filter(filters) {
            body["filter"] = filter;
        }

        let response = self
            .request(
                reqwest::Method::POST,
                &format!("collections/{}/points/search", self.collection),
            )
            .json(&body)
            .send()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SearchError::Index(format!(
                "Qdrant search failed with HTTP {status}: {body}"
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|error| SearchError::Index(error.to_string()))?;
        let results = body
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                SearchError::Index("Qdrant search response missing result array".to_string())
            })?;

        results.iter().map(indexed_result_from_qdrant).collect()
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

#[derive(Debug, Default, Clone)]
pub struct InlinePayloadDocumentStore;

#[async_trait]
impl DocumentStore for InlinePayloadDocumentStore {
    async fn reset(&self) -> Result<(), SearchError> {
        Ok(())
    }

    async fn upsert_documents(&self, documents: Vec<SearchRecord>) -> Result<usize, SearchError> {
        Ok(documents.len())
    }

    async fn fetch_documents(&self, _ids: &[String]) -> Result<Vec<SearchRecord>, SearchError> {
        Ok(Vec::new())
    }
}

fn payload_from_point(point: &IndexedPoint) -> Value {
    serde_json::json!({
        "source": point.source,
        "source_id": point.source_id,
        "title": point.title,
        "text": point.text,
        "lang": point.lang,
        "market": point.market,
        "metadata": point.metadata,
    })
}

fn build_qdrant_filter(filters: &SearchFilters) -> Option<Value> {
    let mut must = Vec::new();

    if let Some(source) = &filters.source {
        must.push(serde_json::json!({
            "key": "source",
            "match": { "value": source }
        }));
    }

    for (key, value) in &filters.metadata {
        let payload_key = match key.as_str() {
            "lang" | "market" | "source_id" | "title" => key.clone(),
            _ => format!("metadata.{key}"),
        };
        must.push(serde_json::json!({
            "key": payload_key,
            "match": { "value": value }
        }));
    }

    if must.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "must": must }))
    }
}

fn indexed_result_from_qdrant(value: &Value) -> Result<IndexedResult, SearchError> {
    let id = qdrant_id_to_string(
        value
            .get("id")
            .ok_or_else(|| SearchError::Index("Qdrant result missing id".to_string()))?,
    )?;
    let score = value
        .get("score")
        .and_then(Value::as_f64)
        .ok_or_else(|| SearchError::Index("Qdrant result missing score".to_string()))?
        as f32;
    let record = value
        .get("payload")
        .map(|payload| search_record_from_payload(payload, &id))
        .transpose()?;

    Ok(IndexedResult { id, score, record })
}

fn qdrant_id_to_string(value: &Value) -> Result<String, SearchError> {
    if let Some(id) = value.as_str() {
        return Ok(id.to_string());
    }
    if let Some(id) = value.as_i64() {
        return Ok(id.to_string());
    }
    if let Some(id) = value
        .get("uuid")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    {
        return Ok(id);
    }

    Err(SearchError::Index(
        "Unsupported Qdrant point id shape".to_string(),
    ))
}

fn search_record_from_payload(payload: &Value, id: &str) -> Result<SearchRecord, SearchError> {
    let metadata = payload
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    Ok(SearchRecord {
        id: id.to_string(),
        source: required_payload_string(payload, "source")?,
        source_id: required_payload_string(payload, "source_id")?,
        title: payload
            .get("title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        text: required_payload_string(payload, "text")?,
        lang: payload
            .get("lang")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        market: payload
            .get("market")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        metadata,
    })
}

fn required_payload_string(payload: &Value, key: &str) -> Result<String, SearchError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| SearchError::Index(format!("Qdrant payload missing '{key}'")))
}

async fn parse_embedding_response(response: reqwest::Response) -> Result<Vec<f32>, SearchError> {
    let body = parse_embedder_body(response).await?;
    body.get("embedding")
        .or_else(|| {
            body.get("data")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("embedding"))
        })
        .map(parse_vector)
        .transpose()?
        .ok_or_else(|| {
            SearchError::Embedder("embedder response missing embedding vector".to_string())
        })
}

async fn parse_embeddings_response(
    response: reqwest::Response,
) -> Result<Vec<Vec<f32>>, SearchError> {
    let body = parse_embedder_body(response).await?;

    if let Some(items) = body.get("embeddings").and_then(Value::as_array) {
        return items.iter().map(parse_vector).collect();
    }

    if let Some(items) = body.get("data").and_then(Value::as_array) {
        return items
            .iter()
            .map(|item| {
                item.get("embedding")
                    .ok_or_else(|| {
                        SearchError::Embedder(
                            "embedder response data item missing embedding".to_string(),
                        )
                    })
                    .and_then(parse_vector)
            })
            .collect();
    }

    Err(SearchError::Embedder(
        "embedder response missing embeddings array".to_string(),
    ))
}

async fn parse_embedder_body(response: reqwest::Response) -> Result<Value, SearchError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(SearchError::Embedder(format!(
            "embedder returned HTTP {status}: {body}"
        )));
    }

    response
        .json()
        .await
        .map_err(|error| SearchError::Embedder(error.to_string()))
}

fn parse_vector(value: &Value) -> Result<Vec<f32>, SearchError> {
    value
        .as_array()
        .ok_or_else(|| SearchError::Embedder("embedding value was not an array".to_string()))?
        .iter()
        .map(|component| {
            component.as_f64().map(|value| value as f32).ok_or_else(|| {
                SearchError::Embedder("embedding component was not numeric".to_string())
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{build_qdrant_filter, parse_vector};
    use crate::contracts::SearchFilters;
    use serde_json::json;

    #[test]
    fn qdrant_filter_maps_source_lang_market_and_metadata() {
        let filter = build_qdrant_filter(&SearchFilters {
            source: Some("merchant_catalog".to_string()),
            metadata: serde_json::Map::from_iter([
                ("lang".to_string(), json!("wo")),
                ("market".to_string(), json!("SN-DKR")),
                ("merchant".to_string(), json!("Awa Textiles")),
            ]),
        })
        .expect("filter");

        let must = filter["must"].as_array().expect("must");
        assert_eq!(must.len(), 4);
        assert!(must.iter().any(|entry| entry["key"] == json!("source")));
        assert!(must.iter().any(|entry| entry["key"] == json!("lang")));
        assert!(must.iter().any(|entry| entry["key"] == json!("market")));
        assert!(must
            .iter()
            .any(|entry| entry["key"] == json!("metadata.merchant")));
    }

    #[test]
    fn parse_vector_rejects_non_numeric_components() {
        let error = parse_vector(&json!(["bad"])).expect_err("expected numeric parse failure");
        assert!(error.to_string().contains("embedding component"));
    }
}
