//! Model adapter implementations for the Yaatal AI harness.
//!
//! This crate contains concrete adapters for various model types such as
//! embeddings, ranking models or classifiers. These adapters adhere to
//! the [`ModelAdapter`](yaatal_core::ModelAdapter) trait defined in
//! `yaatal-core`, enabling them to be swapped out or mocked in tests.
//!
//! Also provides LLM provider implementations (OpenAI, Anthropic, etc.)
//! built on top of the [`LlmProvider`](yaatal_core::LlmProvider) trait.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;
use yaatal_core::{
    ChatParams, HarnessError, LlmError, LlmProvider, LlmResponse, Message, ModelAdapter,
    RequestContext, TokenUsage, ToolCall,
};

// =============================================================================
// DUMMY/LEGACY ADAPTERS (for backwards compatibility)
// =============================================================================

/// A trivial embedding model that returns the length of the input string
/// as its output. Replace this with a call to a real embedding service.
pub struct DummyEmbedder;

#[async_trait]
impl ModelAdapter<String, Vec<f32>> for DummyEmbedder {
    async fn infer(&self, ctx: &RequestContext, input: String) -> Result<Vec<f32>, HarnessError> {
        info!(request_id = %ctx.request_id, input = %input, "dummy_embedder");
        // Return a vector containing the length of the input string as a float.
        Ok(vec![input.len() as f32])
    }
}

/// A stub ranking model that assigns descending scores based on the
/// position of items. In a real system this would invoke a learned
/// model or remote ranking service.
pub struct DummyRankingModel;

#[async_trait]
impl ModelAdapter<Vec<String>, Vec<f32>> for DummyRankingModel {
    async fn infer(
        &self,
        ctx: &RequestContext,
        input: Vec<String>,
    ) -> Result<Vec<f32>, HarnessError> {
        info!(request_id = %ctx.request_id, num_inputs = input.len(), "dummy_ranker");
        // Assign scores decreasing from 1.0 to 0.0 based on index.
        let n = input.len();
        let scores: Vec<f32> = (0..n).map(|i| 1.0 - (i as f32 / n as f32)).collect();
        Ok(scores)
    }
}

// =============================================================================
// OPENAI PROVIDER (Extracted from zeroclaw-providers)
// =============================================================================

/// OpenAI API client for chat completions and embeddings.
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    default_model: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".to_string(),
            default_model: "gpt-4o".to_string(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            default_model: model.into(),
        }
    }

    async fn call_chat_api(
        &self,
        messages: &[Message],
        params: &ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        use reqwest::Client;

        let client = Client::new();
        let url = format!("{}/chat/completions", self.base_url);

        // Convert messages to OpenAI format
        let openai_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    yaatal_core::MessageRole::System => "system",
                    yaatal_core::MessageRole::User => "user",
                    yaatal_core::MessageRole::Assistant => "assistant",
                    yaatal_core::MessageRole::ToolResult => "tool",
                };
                let mut msg = serde_json::json!({
                    "role": role,
                    "content": m.content,
                });
                if let Some(name) = &m.name {
                    msg["name"] = serde_json::json!(name);
                }
                msg
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.default_model,
            "messages": openai_messages,
        });

        if let Some(max_tokens) = params.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = params.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = params.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &params.stop {
            body["stop"] = serde_json::json!(stop);
        }

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 401 {
                return Err(LlmError::AuthenticationFailed);
            }
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimit);
            }
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let content = response_body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tool_calls: Vec<ToolCall> = response_body["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|tc| ToolCall {
                        name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                        arguments: tc["function"]["arguments"].to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = response_body
            .get("usage")
            .filter(|u| !u.is_null())
            .map(|u| TokenUsage {
                prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
            });

        Ok(LlmResponse {
            content,
            tool_calls,
            usage,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn provider_name(&self) -> &str {
        "openai"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "gpt-4o".to_string(),
            "gpt-4-turbo".to_string(),
            "gpt-4".to_string(),
            "gpt-3.5-turbo".to_string(),
            "text-embedding-3-small".to_string(),
            "text-embedding-3-large".to_string(),
            "text-embedding-ada-002".to_string(),
        ]
    }

    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        params: ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        info!(
            request_id = %ctx.request_id,
            model = %self.default_model,
            num_messages = messages.len(),
            "openai_chat"
        );
        self.call_chat_api(messages, &params).await
    }

    async fn embed(
        &self,
        ctx: &RequestContext,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError> {
        use reqwest::Client;

        info!(
            request_id = %ctx.request_id,
            num_texts = texts.len(),
            "openai_embed"
        );

        let client = Client::new();
        let url = format!("{}/embeddings", self.base_url);

        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": texts,
        });

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "Embeddings API error: {}",
                body
            )));
        }

        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let embeddings: Vec<Vec<f32>> = response_body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        item["embedding"]
                            .as_array()
                            .map(|v| {
                                v.iter()
                                    .filter_map(|f| f.as_f64())
                                    .map(|f| f as f32)
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(embeddings)
    }

    async fn health_check(&self) -> bool {
        use reqwest::Client;
        let client = Client::new();
        let url = format!("{}/models", self.base_url);

        client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

// =============================================================================
// ANTHROPIC PROVIDER
// =============================================================================

/// Anthropic API client for Claude models.
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    default_model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            default_model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    async fn call_chat_api(
        &self,
        messages: &[Message],
        params: &ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        use reqwest::Client;

        let client = Client::new();
        let url = format!("{}/messages", self.base_url);

        // Convert messages to Anthropic format
        let anthropic_messages: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| m.role != yaatal_core::MessageRole::System) // System goes in system param
            .map(|m| {
                let role = match m.role {
                    yaatal_core::MessageRole::User => "user",
                    yaatal_core::MessageRole::Assistant => "assistant",
                    yaatal_core::MessageRole::ToolResult => "user", // Claude uses user role for tool results
                    _ => "user",
                };
                serde_json::json!({
                    "role": role,
                    "content": m.content,
                })
            })
            .collect();

        // Extract system message if present
        let system_msg = messages
            .iter()
            .find(|m| m.role == yaatal_core::MessageRole::System)
            .map(|m| m.content.clone());

        let mut body = serde_json::json!({
            "model": self.default_model,
            "messages": anthropic_messages,
            "max_tokens": params.max_tokens.unwrap_or(1024),
        });

        if let Some(system) = system_msg {
            body["system"] = serde_json::json!(system);
        }
        if let Some(temp) = params.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = params.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }

        let response = client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 401 {
                return Err(LlmError::AuthenticationFailed);
            }
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimit);
            }
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "Anthropic API error {}: {}",
                status, body
            )));
        }

        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let content = response_body["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = TokenUsage {
            prompt_tokens: response_body["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: response_body["usage"]["output_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            total_tokens: 0, // Anthropic doesn't always return this
        };

        Ok(LlmResponse {
            content,
            tool_calls: vec![],
            usage: Some(usage),
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "claude-opus-4-20250514".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-opus-20240229".to_string(),
            "claude-3-sonnet-20240229".to_string(),
        ]
    }

    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        params: ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        info!(
            request_id = %ctx.request_id,
            model = %self.default_model,
            num_messages = messages.len(),
            "anthropic_chat"
        );
        self.call_chat_api(messages, &params).await
    }

    async fn embed(
        &self,
        _ctx: &RequestContext,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError> {
        // Anthropic doesn't have a standalone embedding API
        Err(LlmError::ModelNotAvailable("embeddings".to_string()))
    }

    async fn health_check(&self) -> bool {
        use reqwest::Client;
        let client = Client::new();
        let url = format!("{}/messages", self.base_url);

        client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.default_model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

// =============================================================================
// FALLBACK ROUTER (Extracted from zeroclaw-providers)
// =============================================================================

/// A provider that routes to the first available provider in a list.
/// Falls back to subsequent providers on failure.
pub struct FallbackRouter {
    providers: Vec<Arc<dyn LlmProvider>>,
}

impl FallbackRouter {
    /// Create a new fallback router with the given providers.
    pub fn new(providers: Vec<Arc<dyn LlmProvider>>) -> Self {
        Self { providers }
    }

    /// Add a fallback provider.
    pub fn with_fallback(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.providers.push(provider);
        self
    }
}

#[async_trait]
impl LlmProvider for FallbackRouter {
    fn provider_name(&self) -> &str {
        "fallback_router"
    }

    fn default_model(&self) -> &str {
        self.providers
            .first()
            .map(|p| p.default_model())
            .unwrap_or("unknown")
    }

    fn available_models(&self) -> Vec<String> {
        self.providers
            .iter()
            .flat_map(|p| p.available_models())
            .collect()
    }

    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        params: ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        let mut last_error = LlmError::Provider("No providers available".to_string());

        for provider in &self.providers {
            match provider.chat(ctx, messages, params.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        provider = %provider.provider_name(),
                        error = %e,
                        "Provider failed, trying next fallback"
                    );
                    last_error = e;
                }
            }
        }

        Err(last_error)
    }

    async fn embed(
        &self,
        ctx: &RequestContext,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError> {
        let mut last_error = LlmError::Provider("No providers available".to_string());

        for provider in &self.providers {
            match provider.embed(ctx, texts).await {
                Ok(embeddings) => return Ok(embeddings),
                Err(e) => {
                    tracing::warn!(
                        provider = %provider.provider_name(),
                        error = %e,
                        "Embedding provider failed, trying next"
                    );
                    last_error = e;
                }
            }
        }

        Err(last_error)
    }

    async fn health_check(&self) -> bool {
        // Router is healthy if any provider is healthy
        for provider in &self.providers {
            if provider.health_check().await {
                return true;
            }
        }
        false
    }
}

// =============================================================================
// ADAPTER WRAPPERS (Bridge trait to LlmProvider)
// =============================================================================

/// Embedding adapter that wraps an LLM provider.
pub struct ProviderEmbeddingAdapter<P: LlmProvider> {
    provider: Arc<P>,
}

impl<P: LlmProvider> ProviderEmbeddingAdapter<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: LlmProvider + Send + Sync> ModelAdapter<String, Vec<f32>> for ProviderEmbeddingAdapter<P> {
    async fn infer(&self, ctx: &RequestContext, input: String) -> Result<Vec<f32>, HarnessError> {
        let embeddings = self
            .provider
            .embed(ctx, &[input])
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
}

/// Ranking adapter that wraps an LLM provider for re-ranking.
pub struct ProviderRankingAdapter<P: LlmProvider> {
    provider: Arc<P>,
}

impl<P: LlmProvider> ProviderRankingAdapter<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: LlmProvider + Send + Sync> ModelAdapter<Vec<String>, Vec<f32>>
    for ProviderRankingAdapter<P>
{
    async fn infer(
        &self,
        ctx: &RequestContext,
        inputs: Vec<String>,
    ) -> Result<Vec<f32>, HarnessError> {
        // Build a prompt for ranking
        let candidates = inputs
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "Rate the relevance of each item below (0.0 to 1.0):\n{}\n\nRespond with scores only, one per line.",
            candidates
        );

        let messages = vec![Message::user(&prompt)];

        let params = ChatParams {
            max_tokens: Some(inputs.len() as u32 * 10),
            ..Default::default()
        };

        let response = self
            .provider
            .chat(ctx, &messages, params)
            .await
            .map_err(|e| HarnessError::External(e.to_string()))?;

        // Parse scores from response
        let scores: Vec<f32> = response
            .content
            .lines()
            .filter_map(|line| line.trim().parse::<f32>().ok())
            .take(inputs.len())
            .collect();

        // If parsing failed, return uniform scores
        if scores.len() != inputs.len() {
            let n = inputs.len();
            return Ok((0..n).map(|i| 1.0 - (i as f32 / n as f32)).collect());
        }

        Ok(scores)
    }
}

// =============================================================================
// MOCK PROVIDER (For testing)
// =============================================================================

use std::collections::HashMap;

/// Mock LLM provider for testing.
/// Allows deterministic responses based on configured behavior.
pub struct MockProvider {
    /// Pre-configured responses keyed by input pattern
    responses: HashMap<String, LlmResponse>,
    /// Default response if no pattern matches
    default_response: LlmResponse,
    /// Call counter
    call_count: std::sync::atomic::AtomicUsize,
    /// Responses to return in order (FIFO)
    response_queue: std::sync::Mutex<Vec<LlmResponse>>,
}

impl MockProvider {
    /// Create a new mock provider with default response.
    pub fn new(default_content: impl Into<String>) -> Self {
        Self {
            responses: HashMap::new(),
            default_response: LlmResponse {
                content: default_content.into(),
                tool_calls: vec![],
                usage: Some(TokenUsage::default()),
            },
            call_count: std::sync::atomic::AtomicUsize::new(0),
            response_queue: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Add a response for a specific input pattern.
    pub fn with_response(mut self, pattern: impl Into<String>, response: LlmResponse) -> Self {
        self.responses.insert(pattern.into(), response);
        self
    }

    /// Add a simple text response for a pattern.
    pub fn with_text(mut self, pattern: impl Into<String>, content: impl Into<String>) -> Self {
        self.responses.insert(
            pattern.into(),
            LlmResponse {
                content: content.into(),
                tool_calls: vec![],
                usage: Some(TokenUsage::default()),
            },
        );
        self
    }

    /// Add a tool call response for a pattern.
    pub fn with_tool_call(
        mut self,
        pattern: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        self.responses.insert(
            pattern.into(),
            LlmResponse {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    name: tool_name.into(),
                    arguments: arguments.into(),
                }],
                usage: Some(TokenUsage::default()),
            },
        );
        self
    }

    /// Queue responses to return in order (FIFO).
    pub fn with_queue(self, responses: Vec<LlmResponse>) -> Self {
        *self.response_queue.lock().unwrap() = responses;
        self
    }

    /// Get the number of times chat was called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Reset the call counter.
    pub fn reset_count(&self) {
        self.call_count
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new("Mock response")
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn provider_name(&self) -> &str {
        "mock"
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    fn available_models(&self) -> Vec<String> {
        vec!["mock-model".to_string()]
    }

    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        _params: ChatParams,
    ) -> Result<LlmResponse, LlmError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Build input string from messages
        let input = messages
            .iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        tracing::debug!(request_id = %ctx.request_id, input = %input, "MockProvider chat");

        // Check queue first
        if let Ok(mut queue) = self.response_queue.lock() {
            if let Some(response) = queue.pop() {
                return Ok(response);
            }
        }

        // Check for matching pattern
        for (pattern, response) in &self.responses {
            if input.contains(pattern) {
                return Ok(response.clone());
            }
        }

        // Return default
        Ok(self.default_response.clone())
    }

    async fn embed(
        &self,
        _ctx: &RequestContext,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError> {
        // Return simple embedding: hash of text as single dimension
        let embeddings: Vec<Vec<f32>> = texts
            .iter()
            .map(|t| vec![t.len() as f32]) // Simple length-based embedding
            .collect();
        Ok(embeddings)
    }

    async fn health_check(&self) -> bool {
        true
    }
}
