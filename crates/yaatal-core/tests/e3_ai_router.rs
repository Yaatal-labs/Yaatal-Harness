#![allow(clippy::unwrap_used, clippy::expect_used)]
//! E3 — AI cascade router deterministic tests.
//!
//! These tests use mock network gates and no real HTTP calls.
//! They verify offline/2G gating, rate limiting, sensitivity-aware
//! routing, latency tracking, and the `Result`-returning constructor.

use yaatal_core::ai::classify::{classify_task, AiTask};
use yaatal_core::ai::network::{NetworkCondition, NetworkGate};
use yaatal_core::ai::rate_limit::RateLimiterPool;
use yaatal_core::ai::router::{AiConfig, AiRouter, KeyField, TierConfig};

// ── Helpers ─────────────────────────────────────────────────────────

/// A mock network gate that returns a fixed condition.
struct MockGate(NetworkCondition);

impl NetworkGate for MockGate {
    fn condition(&self) -> NetworkCondition {
        self.0
    }
}

/// An empty AI config (no keys set).
fn empty_config() -> AiConfig {
    AiConfig {
        siliconflow_key: None,
        huggingface_key: None,
        openrouter_key: None,
        anthropic_key: None,
    }
}

/// A minimal tier cascade with only the on-device tier.
fn tier1_only() -> Vec<TierConfig> {
    vec![TierConfig {
        tier: 1,
        name: "on-device",
        endpoint: "",
        model: "local-placeholder",
        min_network: NetworkCondition::Offline,
        sensitive_capable: false,
        rate_limit_rpm: 0,
        key_field: KeyField::None,
    }]
}

/// Two tiers: T1 (on-device, always), T2 (remote, requires 3G+).
fn two_tier_cascade() -> Vec<TierConfig> {
    vec![
        TierConfig {
            tier: 1,
            name: "on-device",
            endpoint: "",
            model: "local-placeholder",
            min_network: NetworkCondition::Offline,
            sensitive_capable: false,
            rate_limit_rpm: 0,
            key_field: KeyField::None,
        },
        TierConfig {
            tier: 2,
            name: "remote-test",
            endpoint: "https://example.invalid/v1/chat/completions",
            model: "test-model",
            min_network: NetworkCondition::ThreeG,
            sensitive_capable: true,
            rate_limit_rpm: 60,
            key_field: KeyField::SiliconFlow,
        },
    ]
}

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn route_offline_returns_tier1_only() {
    let router = AiRouter::with_options(
        empty_config(),
        two_tier_cascade(),
        Box::new(MockGate(NetworkCondition::Offline)),
    )
    .expect("build router");

    let resp = router.route(&[], "hello").await.expect("should succeed");
    assert_eq!(resp.tier_used, 1, "offline should only use tier 1");
    assert!(
        resp.content.contains("on-device"),
        "tier 1 placeholder should mention on-device"
    );
}

#[tokio::test]
async fn route_2g_returns_tier1_only() {
    let router = AiRouter::with_options(
        empty_config(),
        two_tier_cascade(),
        Box::new(MockGate(NetworkCondition::TwoG)),
    )
    .expect("build router");

    let resp = router
        .route(&[], "tell me a joke")
        .await
        .expect("should succeed");
    assert_eq!(resp.tier_used, 1, "2G should only use tier 1");
}

#[tokio::test]
async fn route_sensitive_skips_non_capable_tiers() {
    // Both tiers: T1 (not sensitive_capable), T2 (sensitive_capable but no key).
    // Sensitive query → skip T1, try T2, T2 has no key → AllTiersExhausted.
    let router = AiRouter::with_options(
        empty_config(),
        two_tier_cascade(),
        Box::new(MockGate(NetworkCondition::FourGPlus)),
    )
    .expect("build router");

    // "blockchain" triggers sensitivity
    let result = router.route(&[], "explain blockchain privacy").await;
    assert!(
        result.is_err(),
        "sensitive query should exhaust tiers when T2 has no key"
    );
}

#[tokio::test]
async fn route_non_sensitive_uses_tier1() {
    let router = AiRouter::with_options(
        empty_config(),
        two_tier_cascade(),
        Box::new(MockGate(NetworkCondition::FourGPlus)),
    )
    .expect("build router");

    // non-sensitive, T1 always available → should hit tier 1
    let resp = router
        .route(&[], "how are you?")
        .await
        .expect("should succeed");
    assert_eq!(resp.tier_used, 1);
}

#[tokio::test]
async fn route_all_tiers_exhausted_when_no_keys() {
    // Only remote tiers, no keys, non-sensitive
    let tiers = vec![TierConfig {
        tier: 2,
        name: "remote-only",
        endpoint: "https://example.invalid/v1/chat/completions",
        model: "test-model",
        min_network: NetworkCondition::ThreeG,
        sensitive_capable: true,
        rate_limit_rpm: 0,
        key_field: KeyField::SiliconFlow,
    }];
    let router = AiRouter::with_options(
        empty_config(),
        tiers,
        Box::new(MockGate(NetworkCondition::FourGPlus)),
    )
    .expect("build router");

    let result = router.route(&[], "hello").await;
    assert!(result.is_err(), "no API keys → all tiers exhausted");
}

#[tokio::test]
async fn route_latency_is_populated() {
    let router = AiRouter::with_options(
        empty_config(),
        tier1_only(),
        Box::new(MockGate(NetworkCondition::FourGPlus)),
    )
    .expect("build router");

    let resp = router.route(&[], "hi").await.expect("should succeed");
    // Tier 1 is instant, so latency might be 0ms on fast machines,
    // but the field must be present and not a sentinel like u64::MAX.
    assert!(resp.latency_ms < 1000, "latency should be reasonable");
}

#[tokio::test]
async fn constructor_returns_result() {
    let result = AiRouter::new(empty_config());
    assert!(
        result.is_ok(),
        "constructor should return Ok with valid config"
    );
}

// ── Classify tests (Wolof keywords) ────────────────────────────────

#[test]
fn classify_default_is_chat() {
    let (task, _) = classify_task("Nanga def?");
    assert_eq!(task, AiTask::Chat, "Wolof greeting should default to Chat");
}

// ── Network condition ordering ──────────────────────────────────────

#[test]
fn network_condition_ordering() {
    assert!(NetworkCondition::Offline < NetworkCondition::TwoG);
    assert!(NetworkCondition::TwoG < NetworkCondition::ThreeG);
    assert!(NetworkCondition::ThreeG < NetworkCondition::FourGPlus);
}

// ── Rate limiter unit tests ─────────────────────────────────────────

#[test]
fn rate_limiter_pool_basics() {
    let mut pool = RateLimiterPool::default();
    pool.register("provider-a", 2);

    assert!(pool.try_acquire("provider-a"));
    assert!(pool.try_acquire("provider-a"));
    assert!(
        !pool.try_acquire("provider-a"),
        "should be rate-limited after exhausting capacity"
    );
    assert!(
        pool.retry_after_ms("provider-a") > 0,
        "retry_after should be positive when exhausted"
    );
}
