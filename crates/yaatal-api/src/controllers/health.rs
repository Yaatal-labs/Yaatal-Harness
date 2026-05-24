use loco_rs::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[debug_handler]
async fn health() -> Result<Response> {
    format::json(HealthResponse {
        status: "ok",
        service: "yaatal-api",
    })
}

pub fn routes() -> Routes {
    Routes::new().add("/health", get(health))
}
