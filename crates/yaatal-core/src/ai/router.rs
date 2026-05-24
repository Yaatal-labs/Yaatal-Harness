//! 5-Tier AI Cascade Router
//!
//! Always starts at Tier 1 (on-device) and cascades up on failure/timeout.
//! On 2G/offline: Tier 1 only. Tiers 2-5 require 3G+.
//!
//! The router is data-driven — tiers are defined in [`TierConfig`] structs
//! rather than hard-coded match arms, making it easy to add/remove providers.

use crate::ai::classify::classify_task;
use crate::ai::network::{DefaultNetworkGate, NetworkCondition, NetworkGate};
use crate::ai::rate_limit::RateLimiterPool;
use crate::ai::sensitivity::is_sensitive;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AiError {
    #[error("All tiers exhausted")]
    AllTiersExhausted,
    #[error("Tier {tier} ({name}) failed: {reason}")]
    TierFailed {
        tier: u8,
        name: String,
        reason: String,
    },
    #[error("Tier {tier} ({name}) rate-limited — retry after {retry_after_ms}ms")]
    RateLimited {
        tier: u8,
        name: String,
        retry_after_ms: u64,
    },
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Timeout after {0:?}")]
    Timeout(Duration),
    #[error("HTTP client build error: {0}")]
    ClientBuild(String),
}

// ── Domain types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub tier_used: u8,
    pub model: String,
    pub task: String,
    pub latency_ms: u64,
}

// ── Tier configuration ──────────────────────────────────────────────

/// Describes a single cascade tier.
#[derive(Debug, Clone)]
pub struct TierConfig {
    /// Tier number (1-5).
    pub tier: u8,
    /// Human-readable name for logging.
    pub name: &'static str,
    /// API endpoint URL.
    pub endpoint: &'static str,
    /// Model identifier sent in the request body.
    pub model: &'static str,
    /// Minimum network condition to attempt this tier.
    pub min_network: NetworkCondition,
    /// Whether this tier can handle sensitive content.
    pub sensitive_capable: bool,
    /// Rate limit in requests per minute (0 = unlimited).
    pub rate_limit_rpm: u32,
    /// Which API key field from [`AiConfig`] to use.
    pub key_field: KeyField,
}

/// Selects which API key from [`AiConfig`] a tier uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyField {
    /// No key needed (on-device tier).
    None,
    SiliconFlow,
    HuggingFace,
    OpenRouter,
    Anthropic,
}

/// Default 5-tier cascade.
pub const DEFAULT_TIERS: &[TierConfig] = &[
    TierConfig {
        tier: 1,
        name: "on-device",
        endpoint: "",
        model: "local-placeholder",
        min_network: NetworkCondition::Offline, // always available
        sensitive_capable: false,
        rate_limit_rpm: 0,
        key_field: KeyField::None,
    },
    TierConfig {
        tier: 2,
        name: "siliconflow-lfm2",
        endpoint: "https://api.siliconflow.cn/v1/chat/completions",
        model: "liquid/lfm-2-1.2b-chat",
        min_network: NetworkCondition::ThreeG,
        sensitive_capable: false,
        rate_limit_rpm: 60,
        key_field: KeyField::SiliconFlow,
    },
    TierConfig {
        tier: 3,
        name: "siliconflow-qwen",
        endpoint: "https://api.siliconflow.cn/v1/chat/completions",
        model: "Qwen/Qwen2.5-72B-Instruct",
        min_network: NetworkCondition::ThreeG,
        sensitive_capable: false,
        rate_limit_rpm: 30,
        key_field: KeyField::SiliconFlow,
    },
    TierConfig {
        tier: 4,
        name: "openrouter-premium",
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        model: "anthropic/claude-sonnet-4-20250514",
        min_network: NetworkCondition::ThreeG,
        sensitive_capable: true,
        rate_limit_rpm: 20,
        key_field: KeyField::OpenRouter,
    },
    TierConfig {
        tier: 5,
        name: "huggingface-mistral",
        endpoint: "https://api-inference.huggingface.co/v1/chat/completions",
        model: "mistralai/Mistral-7B-Instruct-v0.2",
        min_network: NetworkCondition::ThreeG,
        sensitive_capable: false,
        rate_limit_rpm: 30,
        key_field: KeyField::HuggingFace,
    },
];

// ── AI config ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub siliconflow_key: Option<String>,
    pub huggingface_key: Option<String>,
    pub openrouter_key: Option<String>,
    pub anthropic_key: Option<String>,
}

impl AiConfig {
    /// Resolve the API key for a given [`KeyField`].
    fn key_for(&self, field: KeyField) -> Option<&str> {
        match field {
            KeyField::None => None,
            KeyField::SiliconFlow => self.siliconflow_key.as_deref(),
            KeyField::HuggingFace => self.huggingface_key.as_deref(),
            KeyField::OpenRouter => self.openrouter_key.as_deref(),
            KeyField::Anthropic => self.anthropic_key.as_deref(),
        }
    }
}

// ── Router ──────────────────────────────────────────────────────────

pub struct AiRouter {
    client: Client,
    config: AiConfig,
    tiers: Vec<TierConfig>,
    network_gate: Box<dyn NetworkGate>,
    rate_limiters: Mutex<RateLimiterPool>,
}

impl AiRouter {
    /// Create a new router with the default tier cascade and network gate.
    pub fn new(config: AiConfig) -> Result<Self, AiError> {
        Self::with_options(config, DEFAULT_TIERS.to_vec(), Box::new(DefaultNetworkGate))
    }

    /// Create a router with custom tiers and/or network gate (test-friendly).
    pub fn with_options(
        config: AiConfig,
        tiers: Vec<TierConfig>,
        network_gate: Box<dyn NetworkGate>,
    ) -> Result<Self, AiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AiError::ClientBuild(e.to_string()))?;

        let mut pool = RateLimiterPool::default();
        for tier in &tiers {
            if tier.rate_limit_rpm > 0 {
                pool.register(tier.name, tier.rate_limit_rpm);
            }
        }

        Ok(Self {
            client,
            config,
            tiers,
            network_gate,
            rate_limiters: Mutex::new(pool),
        })
    }

    /// Route a query through the AI cascade.
    ///
    /// The router walks tiers in order, skipping those that:
    /// - require better connectivity than currently available,
    /// - cannot handle sensitive content (when the query is sensitive),
    /// - are rate-limited,
    /// - lack a configured API key.
    pub async fn route(
        &self,
        messages: &[Message],
        user_text: &str,
    ) -> Result<AiResponse, AiError> {
        let (task, _reason) = classify_task(user_text);
        let sensitive = is_sensitive(user_text);
        let timeout = task.timeout();
        let network = self.network_gate.condition();

        for tier_cfg in &self.tiers {
            // ── Gate: network condition ──────────────────────────
            if network < tier_cfg.min_network {
                tracing::debug!(
                    "Tier {} ({}) skipped: network {} < required {}",
                    tier_cfg.tier,
                    tier_cfg.name,
                    network,
                    tier_cfg.min_network,
                );
                continue;
            }

            // ── Gate: sensitivity ────────────────────────────────
            if sensitive && !tier_cfg.sensitive_capable {
                tracing::debug!(
                    "Tier {} ({}) skipped: sensitive query, tier not capable",
                    tier_cfg.tier,
                    tier_cfg.name,
                );
                continue;
            }

            // ── Gate: rate limit ─────────────────────────────────
            {
                let mut pool = self
                    .rate_limiters
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !pool.try_acquire(tier_cfg.name) {
                    let retry_ms = pool.retry_after_ms(tier_cfg.name);
                    tracing::warn!(
                        "Tier {} ({}) rate-limited, retry after {}ms",
                        tier_cfg.tier,
                        tier_cfg.name,
                        retry_ms,
                    );
                    continue;
                }
            }

            // ── Tier 1: on-device placeholder ────────────────────
            if tier_cfg.key_field == KeyField::None {
                let start = Instant::now();
                let content = "[on-device: not yet implemented — will use local GGUF model in E7]"
                    .to_string();
                return Ok(AiResponse {
                    content,
                    tier_used: tier_cfg.tier,
                    model: tier_cfg.model.to_string(),
                    task: format!("{:?}", task),
                    latency_ms: start.elapsed().as_millis() as u64,
                });
            }

            // ── Gate: API key present ────────────────────────────
            let api_key = match self.config.key_for(tier_cfg.key_field) {
                Some(k) if !k.is_empty() => k,
                _ => {
                    tracing::debug!(
                        "Tier {} ({}) skipped: no API key configured",
                        tier_cfg.tier,
                        tier_cfg.name,
                    );
                    continue;
                }
            };

            // ── Call the provider ────────────────────────────────
            let start = Instant::now();
            match self
                .call_openai_compatible(
                    tier_cfg.endpoint,
                    api_key,
                    tier_cfg.model,
                    messages,
                    timeout,
                )
                .await
            {
                Ok(content) => {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    return Ok(AiResponse {
                        content,
                        tier_used: tier_cfg.tier,
                        model: tier_cfg.model.to_string(),
                        task: format!("{:?}", task),
                        latency_ms,
                    });
                }
                Err(e) => {
                    tracing::warn!("Tier {} ({}) failed: {}", tier_cfg.tier, tier_cfg.name, e);
                }
            }
        }

        Err(AiError::AllTiersExhausted)
    }

    async fn call_openai_compatible(
        &self,
        endpoint: &str,
        api_key: &str,
        model: &str,
        messages: &[Message],
        timeout: Duration,
    ) -> Result<String, AiError> {
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": 1024,
            "temperature": 0.7,
        });
        let response = self
            .client
            .post(endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&body)
            .send()
            .await?;
        let json: serde_json::Value = response.json().await?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if content.is_empty() {
            return Err(AiError::TierFailed {
                tier: 0,
                name: model.to_string(),
                reason: "Empty response".into(),
            });
        }
        Ok(content)
    }
}
