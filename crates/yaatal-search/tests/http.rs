#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use yaatal_search::{
    contracts::{IndexUpsertRequest, SearchRequest, UpsertDocument},
    http::router,
    memory::{MemoryDocumentStore, MemoryEmbedder, MemoryVectorIndex},
    service::SearchService,
};

type TestService = SearchService<MemoryEmbedder, MemoryVectorIndex, MemoryDocumentStore>;

fn app() -> axum::Router {
    router(Arc::new(TestService::in_memory()))
}

fn metadata(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_string(), value.clone());
    }
    map
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn health_route_returns_ok() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn upsert_then_search_returns_ranked_hits() {
    let service = Arc::new(TestService::in_memory());
    let app = router(Arc::clone(&service));

    let upsert = IndexUpsertRequest {
        documents: vec![
            UpsertDocument {
                id: "doc-1".into(),
                source: "merchant_catalog".into(),
                source_id: "m-1".into(),
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
                source_id: "m-2".into(),
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

    let upsert_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/index/upsert")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&upsert).expect("upsert json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(upsert_response.status(), StatusCode::OK);
    let upsert_body = json_body(upsert_response).await;
    assert_eq!(upsert_body, json!({ "indexed": 2 }));

    let search = SearchRequest {
        query: "white fabric".into(),
        top_k: 2,
        lang: Some("wo".into()),
        market: Some("SN-DKR".into()),
        filters: None,
    };

    let search_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&search).expect("search json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(search_response.status(), StatusCode::OK);
    let body = json_body(search_response).await;
    let hits = body
        .get("hits")
        .and_then(Value::as_array)
        .expect("hits array");

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0]["id"], json!("doc-1"));
    assert_eq!(hits[0]["source"], json!("merchant_catalog"));
    assert_eq!(hits[0]["metadata"]["merchant"], json!("Awa Textiles"));
    assert!(hits[0]["score"].as_f64().expect("score") >= hits[1]["score"].as_f64().expect("score"));
}

#[tokio::test]
async fn search_filters_by_lang_and_market() {
    let service = Arc::new(TestService::in_memory());
    let app = router(Arc::clone(&service));

    let upsert = IndexUpsertRequest {
        documents: vec![
            UpsertDocument {
                id: "wo-doc".into(),
                source: "merchant_catalog".into(),
                source_id: "m-wo".into(),
                title: Some("Wolof result".into()),
                text: "searchable wolof product".into(),
                lang: Some("wo".into()),
                market: Some("SN-DKR".into()),
                metadata: serde_json::Map::new(),
            },
            UpsertDocument {
                id: "fr-doc".into(),
                source: "merchant_catalog".into(),
                source_id: "m-fr".into(),
                title: Some("French result".into()),
                text: "searchable french product".into(),
                lang: Some("fr".into()),
                market: Some("SN-TH".into()),
                metadata: serde_json::Map::new(),
            },
        ],
        reset: true,
    };

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/index/upsert")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&upsert).expect("upsert json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    let search = SearchRequest {
        query: "product".into(),
        top_k: 10,
        lang: Some("wo".into()),
        market: Some("SN-DKR".into()),
        filters: None,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&search).expect("search json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let hits = body
        .get("hits")
        .and_then(Value::as_array)
        .expect("hits array");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], json!("wo-doc"));
}
