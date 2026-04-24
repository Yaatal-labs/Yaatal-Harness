//! Observability helpers.
//!
//! The Yaatal AI harness relies on structured logging and tracing to
//! capture per-stage latencies, error contexts and request identifiers.
//! This crate centralizes configuration of tracing subscribers so
//! services and pipelines throughout the workspace can easily install
//! consistent instrumentation.

use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the tracing subscriber with a sane default filter.
///
/// If the `RUST_LOG` environment variable is set, the filter will
/// respect it. Otherwise, it defaults to `info` level for the harness
/// crates and `warn` for everything else.
pub fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("yaatal=info,yaatal_core=info,warn"));
    fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();
}
