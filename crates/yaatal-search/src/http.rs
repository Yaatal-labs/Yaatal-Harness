use crate::{
    contracts::{HealthResponse, IndexUpsertRequest, SearchRequest},
    errors::SearchError,
    service::SearchService,
    traits::{DocumentStore, Embedder, VectorIndex},
};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::net::TcpListener;

pub fn router<E, I, D>(state: Arc<SearchService<E, I, D>>) -> Router
where
    E: Embedder + 'static,
    I: VectorIndex + 'static,
    D: DocumentStore + 'static,
{
    Router::new()
        .route("/health", get(health::<E, I, D>))
        .route("/search", post(search::<E, I, D>))
        .route("/index/upsert", post(index_upsert::<E, I, D>))
        .with_state(state)
}

pub async fn run<E, I, D>(
    listener: TcpListener,
    state: Arc<SearchService<E, I, D>>,
) -> Result<(), std::io::Error>
where
    E: Embedder + 'static,
    I: VectorIndex + 'static,
    D: DocumentStore + 'static,
{
    axum::serve(listener, router(state)).await
}

async fn health<E, I, D>(
    State(state): State<Arc<SearchService<E, I, D>>>,
) -> Result<Json<HealthResponse>, SearchError>
where
    E: Embedder + 'static,
    I: VectorIndex + 'static,
    D: DocumentStore + 'static,
{
    state.health().await?;
    Ok(Json(HealthResponse { status: "ok" }))
}

async fn search<E, I, D>(
    State(state): State<Arc<SearchService<E, I, D>>>,
    Json(payload): Json<SearchRequest>,
) -> Result<Json<crate::contracts::SearchResponse>, SearchError>
where
    E: Embedder + 'static,
    I: VectorIndex + 'static,
    D: DocumentStore + 'static,
{
    state.search(payload).await.map(Json)
}

async fn index_upsert<E, I, D>(
    State(state): State<Arc<SearchService<E, I, D>>>,
    Json(payload): Json<IndexUpsertRequest>,
) -> Result<Json<crate::contracts::IndexUpsertResponse>, SearchError>
where
    E: Embedder + 'static,
    I: VectorIndex + 'static,
    D: DocumentStore + 'static,
{
    state.upsert(payload).await.map(Json)
}
