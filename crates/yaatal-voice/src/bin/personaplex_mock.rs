use std::{env, sync::Arc};

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use yaatal_voice::{mock::MockVoiceBackend, server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bind = env::var("VOICE_SERVICE_BIND").unwrap_or_else(|_| "127.0.0.1:8082".to_string());
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(
        bind = %bind,
        backend = "mock-personaplex",
        "voice service starting"
    );

    server::run(listener, Arc::new(MockVoiceBackend::default())).await?;
    Ok(())
}
