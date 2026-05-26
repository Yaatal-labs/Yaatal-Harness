//! Yaatal Analytics — server-side product analytics abstraction.
//!
//! # Architecture
//! Mirrors the StorageDispatcher pattern (Lane 6). A single [`AnalyticsDispatcher`]
//! holds one [`AnalyticsSink`] backend chosen at startup:
//!
//! - [`PostHogSink`] — sends events to PostHog via the blocking HTTP client.
//! - [`LogSink`]    — writes events as `tracing::info!` lines; default when no key set.
//! - [`MockSink`]   — captures events in memory; use in tests.
//!
//! # Usage in dev mode (LogSink output)
//! ```text
//! INFO  yaatal_analytics: analytics_event name="user.registered" distinct_id="usr_abc123" properties={"user_pid":"usr_abc123"}
//! ```
//!
//! # Env vars
//! - `POSTHOG_API_KEY` — if set, activates `PostHogSink`.
//! - `POSTHOG_API_HOST` — optional; overrides the PostHog ingestion host
//!   (e.g. `https://eu.i.posthog.com` for EU region, or your self-hosted instance).

use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::info;

// ─── Public event type ────────────────────────────────────────────────────────

/// A single analytics event.
#[derive(Debug, Clone)]
pub struct AnalyticsEvent {
    /// Short dot-namespaced event name, e.g. `"payment.escrow_released"`.
    pub name: &'static str,
    /// Stable user identifier, or `"system"` for backend-only events.
    pub distinct_id: String,
    /// Arbitrary JSON properties attached to the event.
    pub properties: Value,
}

// ─── Sink trait ───────────────────────────────────────────────────────────────

/// Backend abstraction for delivering analytics events.
///
/// Implementations must be `Send + Sync` so they can live behind an `Arc`.
pub trait AnalyticsSink: Send + Sync {
    /// Fire-and-forget capture. Errors are swallowed — analytics must never
    /// crash the calling code path.
    fn capture(&self, event: AnalyticsEvent);
}

// ─── PostHogSink ──────────────────────────────────────────────────────────────

/// Sends events to PostHog using the blocking HTTP client from `posthog-rs`.
///
/// Self-hosted support: pass `api_host` (e.g. `"https://posthog.acme.com"`)
/// to route events to your own instance. `posthog-rs 0.7.1` routes arbitrary
/// hosts through `ClientOptions::host` → `EndpointManager::determine_server_host`,
/// which passes custom domains through unchanged, so self-hosted works correctly.
///
/// # TODO(perf)
/// Currently issues one HTTP POST per event (`/i/v0/e/`). For high-throughput
/// workloads, migrate to `Client::capture_batch` with an internal queue/flush
/// strategy to amortise per-request overhead.
pub struct PostHogSink {
    client: posthog_rs::Client,
}

impl PostHogSink {
    /// Construct a `PostHogSink`.
    ///
    /// - `api_key`  — PostHog project API key (required).
    /// - `api_host` — optional ingestion host override; defaults to the PostHog
    ///   US cloud endpoint (`https://us.i.posthog.com`).
    pub fn new(api_key: &str, api_host: Option<String>) -> Self {
        use posthog_rs::ClientOptionsBuilder;

        let mut builder = ClientOptionsBuilder::default();
        builder.api_key(api_key.to_string());
        if let Some(host) = api_host {
            builder.host(host);
        }
        let options = builder
            .build()
            .expect("api_key is always set; builder is infallible");
        let client = posthog_rs::client(options);
        Self { client }
    }
}

impl AnalyticsSink for PostHogSink {
    fn capture(&self, event: AnalyticsEvent) {
        let mut ph_event =
            posthog_rs::Event::new(event.name.to_string(), event.distinct_id.clone());

        // Attach each top-level property from the JSON object (if it is an object).
        if let Value::Object(map) = &event.properties {
            for (k, v) in map {
                if let Err(e) = ph_event.insert_prop(k, v) {
                    tracing::warn!(
                        event = event.name,
                        key = %k,
                        error = %e,
                        "analytics: failed to attach property"
                    );
                }
            }
        }

        if let Err(e) = self.client.capture(ph_event) {
            tracing::warn!(
                event = event.name,
                distinct_id = %event.distinct_id,
                error = %e,
                "analytics: PostHog capture failed (event dropped)"
            );
        }
    }
}

// ─── LogSink ─────────────────────────────────────────────────────────────────

/// Default no-dependency backend — writes every event as a `tracing::info!` line.
///
/// Example output:
/// ```text
/// INFO  yaatal_analytics: analytics_event name="user.registered" distinct_id="usr_abc123" properties={"user_pid":"usr_abc123"}
/// ```
#[derive(Default)]
pub struct LogSink;

impl AnalyticsSink for LogSink {
    fn capture(&self, event: AnalyticsEvent) {
        info!(
            name = event.name,
            distinct_id = %event.distinct_id,
            properties = %event.properties,
            "analytics_event"
        );
    }
}

// ─── MockSink ────────────────────────────────────────────────────────────────

/// In-memory sink for tests. Captured events are retrievable via [`MockSink::events`].
///
/// # Example
/// ```rust
/// use std::sync::Arc;
/// use yaatal_analytics::{AnalyticsDispatcher, AnalyticsEvent, MockSink};
/// use serde_json::json;
///
/// let mock = Arc::new(MockSink::default());
/// let dispatcher = AnalyticsDispatcher::with_sink(mock.clone());
/// dispatcher.capture(AnalyticsEvent {
///     name: "test.event",
///     distinct_id: "user1".into(),
///     properties: json!({"foo": "bar"}),
/// });
/// assert_eq!(mock.events().len(), 1);
/// ```
#[derive(Default)]
pub struct MockSink {
    events: Mutex<Vec<AnalyticsEvent>>,
}

impl MockSink {
    /// Return a snapshot of all captured events so far.
    pub fn events(&self) -> Vec<AnalyticsEvent> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl AnalyticsSink for MockSink {
    fn capture(&self, event: AnalyticsEvent) {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(event);
    }
}

// ─── Dispatcher ──────────────────────────────────────────────────────────────

/// Central analytics entry-point. Wraps a chosen [`AnalyticsSink`] and is meant
/// to be stored as `Arc<AnalyticsDispatcher>` in app state.
pub struct AnalyticsDispatcher {
    sink: Arc<dyn AnalyticsSink>,
}

impl AnalyticsDispatcher {
    /// Build a dispatcher backed by the given sink. Useful for tests.
    pub fn with_sink(sink: Arc<dyn AnalyticsSink>) -> Self {
        Self { sink }
    }

    /// Build a dispatcher from environment variables. **Never panics** — falls
    /// back to [`LogSink`] when `POSTHOG_API_KEY` is absent or empty so that
    /// server boots are never broken by missing analytics config.
    ///
    /// Reads:
    /// - `POSTHOG_API_KEY` — if set and non-empty, activates [`PostHogSink`].
    /// - `POSTHOG_API_HOST` — optional host override forwarded to [`PostHogSink`].
    pub fn from_env() -> Self {
        match std::env::var("POSTHOG_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
        {
            Some(api_key) => {
                let api_host = std::env::var("POSTHOG_API_HOST")
                    .ok()
                    .filter(|h| !h.trim().is_empty());
                info!(
                    host = api_host.as_deref().unwrap_or("(posthog cloud default)"),
                    "analytics: PostHogSink active"
                );
                Self {
                    sink: Arc::new(PostHogSink::new(&api_key, api_host)),
                }
            }
            None => {
                info!("analytics: POSTHOG_API_KEY not set — falling back to LogSink");
                Self {
                    sink: Arc::new(LogSink),
                }
            }
        }
    }

    /// Capture an event. Delegates to the configured sink and never panics.
    pub fn capture(&self, event: AnalyticsEvent) {
        self.sink.capture(event);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::{AnalyticsDispatcher, AnalyticsEvent, LogSink, MockSink};

    fn make_event(name: &'static str) -> AnalyticsEvent {
        AnalyticsEvent {
            name,
            distinct_id: "user-test-123".into(),
            properties: json!({"key": "value"}),
        }
    }

    #[test]
    fn mock_sink_captures_events() {
        let mock = Arc::new(MockSink::default());
        let dispatcher = AnalyticsDispatcher::with_sink(mock.clone());

        dispatcher.capture(make_event("test.event_one"));
        dispatcher.capture(make_event("test.event_two"));

        let events = mock.events();
        assert_eq!(events.len(), 2, "expected 2 captured events");
        assert_eq!(events[0].name, "test.event_one");
        assert_eq!(events[1].name, "test.event_two");
        assert_eq!(events[0].distinct_id, "user-test-123");
    }

    #[test]
    fn mock_sink_captures_properties() {
        let mock = Arc::new(MockSink::default());
        let dispatcher = AnalyticsDispatcher::with_sink(mock.clone());

        dispatcher.capture(AnalyticsEvent {
            name: "payment.escrow_released",
            distinct_id: "system".into(),
            properties: json!({"order_id": "ord_001", "amount_xof": 5000}),
        });

        let events = mock.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].properties["order_id"], "ord_001");
        assert_eq!(events[0].properties["amount_xof"], 5000);
    }

    #[test]
    fn log_sink_does_not_panic() {
        // LogSink just writes tracing lines — verify it doesn't blow up.
        let dispatcher = AnalyticsDispatcher::with_sink(Arc::new(LogSink));
        dispatcher.capture(make_event("test.log_sink"));
    }

    #[test]
    fn from_env_without_key_falls_back_to_log_sink() {
        // Ensure no POSTHOG_API_KEY is set in this test process.
        std::env::remove_var("POSTHOG_API_KEY");
        // Should not panic:
        let dispatcher = AnalyticsDispatcher::from_env();
        dispatcher.capture(make_event("test.no_key_boot"));
    }
}
