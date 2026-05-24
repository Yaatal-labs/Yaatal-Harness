use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const DIMENSIONS: usize = 64;

#[derive(Debug, Deserialize)]
struct EmbedQueryRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct EmbedDocumentsRequest {
    texts: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct EmbedQueryResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
struct EmbedDocumentsResponse {
    embeddings: Vec<Vec<f32>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bind = std::env::var("BGE_M3_BIND")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8090)));

    let listener = TcpListener::bind(bind).await?;
    tracing::info!(bind = %bind, "mock embedder starting");
    axum::serve(listener, router()).await?;
    Ok(())
}

fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/embed/query", post(embed_query))
        .route("/embed/documents", post(embed_documents))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn embed_query(Json(payload): Json<EmbedQueryRequest>) -> Json<EmbedQueryResponse> {
    Json(EmbedQueryResponse {
        embedding: embed_text(&payload.text),
    })
}

async fn embed_documents(
    Json(payload): Json<EmbedDocumentsRequest>,
) -> Json<EmbedDocumentsResponse> {
    let embeddings = payload.texts.iter().map(|text| embed_text(text)).collect();
    Json(EmbedDocumentsResponse { embeddings })
}

fn embed_text(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; DIMENSIONS];

    for token in text
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
    {
        let mut hash = 0_u64;
        for byte in token.bytes() {
            hash = hash
                .wrapping_mul(1099511628211)
                .wrapping_add(byte as u64 + 1469598103934665603);
        }
        let index = (hash as usize) % DIMENSIONS;
        vector[index] += 1.0;
    }

    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }

    vector
}
