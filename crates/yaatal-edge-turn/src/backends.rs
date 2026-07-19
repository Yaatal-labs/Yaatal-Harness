use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;

use crate::EngineContext;

const MAX_AUDIT_IDENTIFIER_LEN: usize = 96;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalResult {
    pub output: String,
    pub runner_model: Option<String>,
    pub runner_checkpoint: Option<String>,
}

impl ProposalResult {
    pub fn output_only(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            runner_model: None,
            runner_checkpoint: None,
        }
    }

    pub(crate) fn audit_identity(&self) -> Option<String> {
        let model = self.runner_model.as_deref()?;
        let checkpoint = self.runner_checkpoint.as_deref()?;
        if valid_audit_identifier(model) && valid_audit_identifier(checkpoint) {
            Some(format!("{model}@{checkpoint}"))
        } else {
            None
        }
    }
}

fn valid_audit_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_AUDIT_IDENTIFIER_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[async_trait]
pub trait ContextSource: Send + Sync {
    async fn current(&self) -> Result<EngineContext, String>;
}

#[async_trait]
pub trait ProposalBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn propose(&self, prompt: &str) -> Result<ProposalResult, String>;
}

pub struct HttpEngineContextSource {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl HttpEngineContextSource {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, String> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let token = token.into();
        if base_url.is_empty() {
            return Err("YAATAL_ENGINE_URL is required".to_string());
        }
        if token.trim().is_empty() {
            return Err("YAATAL_TOKEN is required".to_string());
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|error| error.to_string())?,
            base_url,
            token,
        })
    }
}

#[async_trait]
impl ContextSource for HttpEngineContextSource {
    async fn current(&self) -> Result<EngineContext, String> {
        self.client
            .get(format!(
                "{}/api/live-sessions/current/products",
                self.base_url
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<EngineContext>()
            .await
            .map_err(|error| error.to_string())
    }
}

pub struct MockProposalBackend {
    output: String,
}

impl MockProposalBackend {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("YAATAL_MOCK_PROPOSAL")
                .unwrap_or_else(|_| r#"{"tool":"none","confidence":1.0}"#.to_string()),
        )
    }
}

#[async_trait]
impl ProposalBackend for MockProposalBackend {
    fn name(&self) -> &str {
        "mock"
    }

    async fn propose(&self, _prompt: &str) -> Result<ProposalResult, String> {
        Ok(ProposalResult::output_only(self.output.clone()))
    }
}

pub struct MinimindHttpBackend {
    client: reqwest::Client,
    base_url: String,
}

impl MinimindHttpBackend {
    pub fn new(base_url: impl Into<String>) -> Result<Self, String> {
        let base_url = base_url.into();
        if base_url.is_empty() {
            return Err("MINIMIND_URL is required".to_string());
        }

        let parsed = reqwest::Url::parse(&base_url)
            .map_err(|error| format!("invalid MINIMIND_URL: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("MINIMIND_URL must use http or https".to_string());
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| "MINIMIND_URL must include a host".to_string())?;
        let ip_host = host
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(host);
        let is_loopback = host.eq_ignore_ascii_case("localhost")
            || ip_host
                .parse::<IpAddr>()
                .map(|address| address.is_loopback())
                .unwrap_or(false);
        if !is_loopback {
            return Err("MINIMIND_URL must target a loopback host".to_string());
        }

        Ok(Self {
            client: reqwest::Client::builder()
                // A live selling turn is dead after a few seconds — fail fast
                // into the model_backend_error deny path instead of holding
                // the stream (Studio's client gives up at 25s).
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|error| error.to_string())?,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }
}

#[derive(Serialize)]
struct MinimindRequest<'a> {
    prompt: &'a str,
    max_new_tokens: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinimindResponse {
    output: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    checkpoint: Option<String>,
}

#[async_trait]
impl ProposalBackend for MinimindHttpBackend {
    fn name(&self) -> &str {
        "minimind-o"
    }

    async fn propose(&self, prompt: &str) -> Result<ProposalResult, String> {
        let response = self
            .client
            .post(format!("{}/v1/propose", self.base_url))
            .json(&MinimindRequest {
                prompt,
                max_new_tokens: 128,
            })
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<MinimindResponse>()
            .await
            .map_err(|error| error.to_string())?;
        Ok(ProposalResult {
            output: response.output,
            runner_model: response.model,
            runner_checkpoint: response.checkpoint,
        })
    }
}
