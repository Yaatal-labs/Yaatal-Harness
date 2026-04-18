use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use yaatal_search::{
    config::{SearchBackendConfig, SearchServiceConfig},
    http,
    service::SearchService,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = SearchServiceConfig::from_env();
    let listener = TcpListener::bind(config.bind).await?;

    let service = match &config.backend {
        SearchBackendConfig::InMemory => {
            tracing::info!(bind = %config.bind, backend = "in-memory", "search service starting");
            Arc::new(SearchService::in_memory())
        }
        SearchBackendConfig::External {
            bge_m3_url,
            qdrant_url,
            database_url,
        } => {
            return Err(format!(
                "SEARCH_BACKEND=external is not implemented yet. Planned stack requires BGE-M3 ({bge_m3_url}), Qdrant ({qdrant_url}), and Postgres ({database_url})."
            )
            .into());
        }
    };

    http::run(listener, service).await?;
    Ok(())
}
