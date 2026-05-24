use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone)]
struct StoredPoint {
    id: String,
    vector: Vec<f32>,
    payload: Value,
}

#[derive(Debug, Default, Clone)]
struct MockQdrantState {
    collections: Arc<Mutex<HashMap<String, HashMap<String, StoredPoint>>>>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    title: &'static str,
    version: &'static str,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bind = std::env::var("QDRANT_BIND")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 6333)));

    let listener = TcpListener::bind(bind).await?;
    let state = MockQdrantState::default();
    tracing::info!(bind = %bind, "mock qdrant starting");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn router(state: MockQdrantState) -> Router {
    Router::new()
        .route("/", get(root))
        .route(
            "/collections/{collection}",
            get(get_collection)
                .put(put_collection)
                .delete(delete_collection),
        )
        .route("/collections/{collection}/points", put(upsert_points))
        .route(
            "/collections/{collection}/points/search",
            post(search_points),
        )
        .with_state(state)
}

async fn root() -> Json<HealthResponse> {
    Json(HealthResponse {
        title: "mock-qdrant",
        version: "dev",
    })
}

async fn get_collection(
    State(state): State<MockQdrantState>,
    Path(collection): Path<String>,
) -> (StatusCode, Json<Value>) {
    let guard = state.collections.lock().await;
    if guard.contains_key(&collection) {
        (
            StatusCode::OK,
            Json(json!({ "result": { "status": "green" } })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "status": "not found", "result": null })),
        )
    }
}

async fn put_collection(
    State(state): State<MockQdrantState>,
    Path(collection): Path<String>,
    Json(_body): Json<Value>,
) -> Json<Value> {
    state
        .collections
        .lock()
        .await
        .entry(collection)
        .or_default();
    Json(json!({ "status": "ok" }))
}

async fn delete_collection(
    State(state): State<MockQdrantState>,
    Path(collection): Path<String>,
) -> Json<Value> {
    state.collections.lock().await.remove(&collection);
    Json(json!({ "status": "ok" }))
}

async fn upsert_points(
    State(state): State<MockQdrantState>,
    Path(collection): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let points = body
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| "points array missing")
        .expect("points array missing");

    let mut guard = state.collections.lock().await;
    let collection_points = guard.entry(collection).or_default();

    for point in points {
        let id = point
            .get("id")
            .and_then(Value::as_str)
            .expect("point id")
            .to_string();
        let vector = point
            .get("vector")
            .and_then(Value::as_array)
            .expect("point vector")
            .iter()
            .map(|value| value.as_f64().expect("vector component") as f32)
            .collect();
        let payload = point.get("payload").cloned().expect("payload");

        collection_points.insert(
            id.clone(),
            StoredPoint {
                id,
                vector,
                payload,
            },
        );
    }

    Json(json!({ "status": "ok" }))
}

async fn search_points(
    State(state): State<MockQdrantState>,
    Path(collection): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let vector: Vec<f32> = body
        .get("vector")
        .and_then(Value::as_array)
        .expect("search vector")
        .iter()
        .map(|value| value.as_f64().expect("vector component") as f32)
        .collect();
    let limit = body.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
    let filter = body.get("filter");

    let guard = state.collections.lock().await;
    let mut results: Vec<Value> = guard
        .get(&collection)
        .into_iter()
        .flat_map(|points| points.values())
        .filter(|point| filter.is_none_or(|filter| matches_filter(&point.payload, filter)))
        .map(|point| {
            json!({
                "id": point.id,
                "score": cosine_similarity(&vector, &point.vector),
                "payload": point.payload,
            })
        })
        .collect();

    results.sort_by(|left, right| {
        let right_score = right["score"].as_f64().unwrap_or_default();
        let left_score = left["score"].as_f64().unwrap_or_default();
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    results.truncate(limit);

    Json(json!({ "result": results }))
}

fn matches_filter(payload: &Value, filter: &Value) -> bool {
    let Some(must) = filter.get("must").and_then(Value::as_array) else {
        return true;
    };

    must.iter().all(|entry| {
        let key = entry.get("key").and_then(Value::as_str).unwrap_or_default();
        let expected = entry
            .get("match")
            .and_then(|match_clause| match_clause.get("value"));
        match (value_at_path(payload, key), expected) {
            (Some(actual), Some(expected)) => actual == expected,
            _ => false,
        }
    })
}

fn value_at_path<'a>(payload: &'a Value, key: &str) -> Option<&'a Value> {
    let mut current = payload;
    for part in key.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;

    for index in 0..len {
        dot += left[index] * right[index];
        left_norm += left[index] * left[index];
        right_norm += right[index] * right[index];
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}
