use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use yaatal_search::{
    config::{SearchBackendConfig, SearchServiceConfig},
    http,
    service::SearchService,
    BgeM3HttpEmbedder, InlinePayloadDocumentStore, QdrantHttpIndex,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = SearchServiceConfig::from_env();
    let listener = TcpListener::bind(config.bind).await?;

    match config.backend {
        SearchBackendConfig::InMemory => {
            tracing::info!(bind = %config.bind, backend = "in-memory", "search service starting");
            let service = Arc::new(SearchService::in_memory());
            http::run(listener, service).await?;
        }
        SearchBackendConfig::External {
            bge_m3_url,
            qdrant_url,
            qdrant_collection,
            qdrant_api_key,
        } => {
            let bge_m3_url = required_env("BGE_M3_URL", bge_m3_url)?;
            let qdrant_url = required_env("QDRANT_URL", qdrant_url)?;
            tracing::info!(
                bind = %config.bind,
                backend = "external",
                qdrant_collection = %qdrant_collection,
                "search service starting"
            );

            let service = Arc::new(SearchService::new(
                BgeM3HttpEmbedder::new(bge_m3_url),
                QdrantHttpIndex::new(qdrant_url, qdrant_collection, qdrant_api_key),
                InlinePayloadDocumentStore,
            ));
            http::run(listener, service).await?;
        }
    }
    Ok(())
}

fn required_env(name: &str, value: String) -> Result<String, Box<dyn std::error::Error>> {
    if value.trim().is_empty() {
        return Err(format!("{name} must be set when SEARCH_BACKEND=external").into());
    }

    Ok(value)
}
