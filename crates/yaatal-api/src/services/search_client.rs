use std::time::Duration;
use thiserror::Error;
use yaatal_search::contracts::{SearchRequest, SearchResponse};

#[derive(Debug, Error)]
pub enum SearchClientError {
    #[error("http client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("search service returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
}

#[derive(Debug, Clone)]
pub struct SearchServiceClient {
    base_url: String,
    http: reqwest::Client,
}

impl SearchServiceClient {
    pub fn from_env() -> Self {
        let base_url = std::env::var("SEARCH_SERVICE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string());
        let timeout_seconds = std::env::var("SEARCH_SERVICE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(5);
        Self::with_timeout(base_url, Duration::from_secs(timeout_seconds))
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_timeout(base_url, Duration::from_secs(5))
    }

    pub fn with_timeout(base_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub async fn search(
        &self,
        request: &SearchRequest,
    ) -> Result<SearchResponse, SearchClientError> {
        let url = format!("{}/search", self.base_url);
        let response = self.http.post(url).json(request).send().await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(SearchClientError::Api { status, message });
        }

        Ok(response.json().await?)
    }
}

pub fn format_grounding(search_response: &SearchResponse) -> Option<String> {
    if search_response.hits.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for (index, hit) in search_response.hits.iter().take(3).enumerate() {
        let merchant = hit
            .metadata
            .get("merchant")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown merchant");
        let price = hit
            .metadata
            .get("price")
            .and_then(|value| value.as_str())
            .unwrap_or("price unavailable");
        let location = hit
            .metadata
            .get("location")
            .and_then(|value| value.as_str())
            .unwrap_or("location unavailable");

        lines.push(format!(
            "{}. {} | {} | {} | {}",
            index + 1,
            merchant,
            hit.text,
            price,
            location
        ));
    }

    Some(format!("Grounding results:\n{}", lines.join("\n")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use serde_json::json;
    use yaatal_search::contracts::{SearchHit, SearchResponse};

    use super::format_grounding;

    #[test]
    fn grounding_formatter_uses_top_hits() {
        let response = SearchResponse {
            hits: vec![SearchHit {
                id: "doc-1".to_string(),
                text: "White basin fabric, 6 yards".to_string(),
                score: 0.93,
                source: "merchant_catalog".to_string(),
                metadata: serde_json::Map::from_iter([
                    ("merchant".to_string(), json!("Awa Textiles")),
                    ("price".to_string(), json!("12000 XOF")),
                    ("location".to_string(), json!("Sandaga")),
                ]),
            }],
        };

        let grounding = format_grounding(&response).expect("grounding");
        assert!(grounding.contains("Awa Textiles"));
        assert!(grounding.contains("12000 XOF"));
        assert!(grounding.contains("Sandaga"));
    }
}
