#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use yaatal_search::{
    contracts::{IndexUpsertRequest, SearchFilters, SearchRequest, UpsertDocument},
    BgeM3HttpEmbedder, InlinePayloadDocumentStore, QdrantHttpIndex, SearchService,
};

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

fn metadata(pairs: &[(&str, Value)]) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_string(), value.clone());
    }
    map
}

async fn spawn_server(router: Router) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let address = listener.local_addr().expect("listener addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve router");
    });
    (format!("http://{}", address), handle)
}

fn embed_text(text: &str) -> Vec<f32> {
    const DIMENSIONS: usize = 64;
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

fn value_at_path<'a>(payload: &'a Value, key: &str) -> Option<&'a Value> {
    let mut current = payload;
    for part in key.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn matches_filter(payload: &Value, filter: &Value) -> bool {
    let Some(must) = filter.get("must").and_then(Value::as_array) else {
        return true;
    };

    must.iter().all(|entry| {
        let key = entry.get("key").and_then(Value::as_str).unwrap_or_default();
        let expected = entry.get("match").and_then(|m| m.get("value"));
        match (value_at_path(payload, key), expected) {
            (Some(actual), Some(expected)) => actual == expected,
            _ => false,
        }
    })
}

async fn embed_query(Json(body): Json<Value>) -> Json<Value> {
    let text = body
        .get("text")
        .and_then(Value::as_str)
        .expect("embed query text");
    Json(json!({ "embedding": embed_text(text) }))
}

async fn embed_documents(Json(body): Json<Value>) -> Json<Value> {
    let texts = body
        .get("texts")
        .and_then(Value::as_array)
        .expect("embed documents texts");
    let embeddings: Vec<Vec<f32>> = texts
        .iter()
        .map(|value| value.as_str().expect("text string"))
        .map(embed_text)
        .collect();

    Json(json!({ "embeddings": embeddings }))
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
        .expect("qdrant points array");

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
            .map(|value| value.as_f64().expect("vector float") as f32)
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
        .map(|value| value.as_f64().expect("vector float") as f32)
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

#[tokio::test]
async fn external_backend_upsert_and_search_roundtrip() {
    let embedder_router = Router::new()
        .route("/embed/query", post(embed_query))
        .route("/embed/documents", post(embed_documents));
    let (embedder_url, _embedder_handle) = spawn_server(embedder_router).await;

    let qdrant_state = MockQdrantState::default();
    let qdrant_router = Router::new()
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
        .with_state(qdrant_state.clone());
    let (qdrant_url, _qdrant_handle) = spawn_server(qdrant_router).await;

    let service = SearchService::new(
        BgeM3HttpEmbedder::new(embedder_url),
        QdrantHttpIndex::new(qdrant_url, "test-collection", None),
        InlinePayloadDocumentStore,
    );

    let upsert = IndexUpsertRequest {
        documents: vec![
            UpsertDocument {
                id: "doc-1".into(),
                source: "merchant_catalog".into(),
                source_id: "merchant-1".into(),
                title: Some("White fabric".into()),
                text: "white fabric at Sandaga market".into(),
                lang: Some("wo".into()),
                market: Some("SN-DKR".into()),
                metadata: metadata(&[
                    ("merchant", json!("Awa Textiles")),
                    ("price", json!("12000 XOF")),
                ]),
            },
            UpsertDocument {
                id: "doc-2".into(),
                source: "merchant_catalog".into(),
                source_id: "merchant-2".into(),
                title: Some("Blue charger".into()),
                text: "blue phone charger downtown".into(),
                lang: Some("wo".into()),
                market: Some("SN-DKR".into()),
                metadata: metadata(&[
                    ("merchant", json!("Tech Plaza")),
                    ("price", json!("3500 XOF")),
                ]),
            },
        ],
        reset: true,
    };

    let indexed = service.upsert(upsert).await.expect("upsert");
    assert_eq!(indexed.indexed, 2);

    let stored = qdrant_state.collections.lock().await;
    let collection = stored.get("test-collection").expect("created collection");
    assert_eq!(collection.len(), 2);
    assert_eq!(
        collection["doc-1"].payload["metadata"]["merchant"],
        json!("Awa Textiles")
    );
    drop(stored);

    let response = service
        .search(SearchRequest {
            query: "white fabric".into(),
            top_k: 5,
            lang: Some("wo".into()),
            market: Some("SN-DKR".into()),
            filters: Some(SearchFilters {
                source: Some("merchant_catalog".into()),
                metadata: metadata(&[
                    ("merchant", json!("Awa Textiles")),
                    ("source_id", json!("merchant-1")),
                ]),
            }),
        })
        .await
        .expect("search");

    assert_eq!(response.hits.len(), 1);
    assert_eq!(response.hits[0].id, "doc-1");
    assert_eq!(response.hits[0].source, "merchant_catalog");
    assert_eq!(response.hits[0].metadata["merchant"], json!("Awa Textiles"));
}
