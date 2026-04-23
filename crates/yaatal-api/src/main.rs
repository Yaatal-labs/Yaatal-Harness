//! Basic API server placeholder.
//!
//! In a real harness this crate would expose HTTP and/or gRPC endpoints
//! and translate inbound requests into pipeline invocations. For now it
//! simply logs that it has started and exits immediately. Use this
//! stub as a starting point for your own HTTP framework integration.

use tracing::info;
use yaatal_core::RequestContext;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting yaatal-api service");
    // Example: create a request context for demonstration.
    let ctx = RequestContext::new("demo-request");
    info!(request_id = %ctx.request_id, "Received a request");
    // In a real implementation you would invoke the search/feed/voice
    // pipelines here based on the request path and parameters.
    info!("yaatal-api service exiting. Replace this stub with your API implementation.");
}
