use crate::{RankedHit, Retriever, SearchDocument, ZeroShotError};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidecarIndexDocument {
    pub id: String,
    pub text: String,
}

impl From<SearchDocument> for SidecarIndexDocument {
    fn from(value: SearchDocument) -> Self {
        Self {
            id: value.id,
            text: value.text,
        }
    }
}

/// Retriever that delegates to the local Python ColBERT sidecar.
#[derive(Debug, Clone)]
pub struct ColbertHttpRetriever {
    base_url: String,
    client: Client,
}

impl ColbertHttpRetriever {
    pub fn new(base_url: impl Into<String>) -> Result<Self, ZeroShotError> {
        Self::with_timeout(base_url, Duration::from_secs(30))
    }

    pub fn with_timeout(
        base_url: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ZeroShotError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| ZeroShotError::Backend(format!("http client build failed: {err}")))?;

        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            client,
        })
    }

    pub fn health(&self) -> Result<(), ZeroShotError> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .map_err(|err| ZeroShotError::Backend(format!("health request failed: {err}")))?;

        if response.status().is_success() {
            return Ok(());
        }

        Err(ZeroShotError::Backend(format!(
            "health check failed with status {}",
            response.status()
        )))
    }

    pub fn index_documents(
        &self,
        documents: &[SidecarIndexDocument],
        reset: bool,
    ) -> Result<usize, ZeroShotError> {
        let url = format!("{}/index", self.base_url);
        let payload = IndexRequest {
            documents: documents.to_vec(),
            reset,
        };

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .map_err(|err| ZeroShotError::Backend(format!("index request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|_| "<unreadable>".to_owned());
            return Err(ZeroShotError::Backend(format!(
                "index request failed with status {status}: {body}"
            )));
        }

        let body: IndexResponse = response
            .json()
            .map_err(|err| ZeroShotError::Backend(format!("invalid index response: {err}")))?;
        Ok(body.indexed)
    }
}

impl Retriever for ColbertHttpRetriever {
    fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<RankedHit>, ZeroShotError> {
        let url = format!("{}/search", self.base_url);
        let payload = SearchRequest { query, top_k };

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .map_err(|err| ZeroShotError::Backend(format!("search request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|_| "<unreadable>".to_owned());
            return Err(ZeroShotError::Backend(format!(
                "search request failed with status {status}: {body}"
            )));
        }

        let body: SearchResponse = response
            .json()
            .map_err(|err| ZeroShotError::Backend(format!("invalid search response: {err}")))?;
        Ok(body
            .hits
            .into_iter()
            .map(|hit| RankedHit {
                doc_id: hit.doc_id,
                score: hit.score,
            })
            .collect())
    }
}

#[derive(Debug, Serialize)]
struct SearchRequest<'a> {
    query: &'a str,
    top_k: usize,
}

#[derive(Debug, Serialize)]
struct IndexRequest {
    documents: Vec<SidecarIndexDocument>,
    reset: bool,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<HitResponse>,
}

#[derive(Debug, Deserialize)]
struct IndexResponse {
    indexed: usize,
}

#[derive(Debug, Deserialize)]
struct HitResponse {
    doc_id: String,
    score: f32,
}
