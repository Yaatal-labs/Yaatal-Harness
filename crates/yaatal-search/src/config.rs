use std::{env, net::SocketAddr};

#[derive(Debug, Clone)]
pub struct SearchServiceConfig {
    pub bind: SocketAddr,
    pub backend: SearchBackendConfig,
}

#[derive(Debug, Clone)]
pub enum SearchBackendConfig {
    InMemory,
    External {
        bge_m3_url: String,
        qdrant_url: String,
        qdrant_collection: String,
        qdrant_api_key: Option<String>,
    },
}

impl Default for SearchServiceConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8081"
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 8081))),
            backend: SearchBackendConfig::InMemory,
        }
    }
}

impl SearchServiceConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(bind) = env::var("SEARCH_BIND") {
            if let Ok(parsed) = bind.parse() {
                config.bind = parsed;
            }
        }

        match env::var("SEARCH_BACKEND")
            .unwrap_or_else(|_| "in-memory".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "external" => {
                let bge_m3_url = env::var("BGE_M3_URL").unwrap_or_default();
                let qdrant_url = env::var("QDRANT_URL").unwrap_or_default();
                let qdrant_collection =
                    env::var("QDRANT_COLLECTION").unwrap_or_else(|_| "yaatal-search".to_string());
                let qdrant_api_key = env::var("QDRANT_API_KEY")
                    .ok()
                    .filter(|value| !value.trim().is_empty());
                config.backend = SearchBackendConfig::External {
                    bge_m3_url,
                    qdrant_url,
                    qdrant_collection,
                    qdrant_api_key,
                };
            }
            _ => {
                config.backend = SearchBackendConfig::InMemory;
            }
        }

        config
    }
}
