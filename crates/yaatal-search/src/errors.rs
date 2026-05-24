use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("top_k must be greater than zero")]
    InvalidTopK,
    #[error("query must not be empty")]
    EmptyQuery,
    #[error("backend error: {0}")]
    Backend(String),
    #[error("store error: {0}")]
    Store(String),
    #[error("index error: {0}")]
    Index(String),
    #[error("embedder error: {0}")]
    Embedder(String),
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    message: String,
}

impl SearchError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidTopK | Self::EmptyQuery => StatusCode::BAD_REQUEST,
            Self::Backend(_) | Self::Store(_) | Self::Index(_) | Self::Embedder(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_name(&self) -> &'static str {
        match self {
            Self::InvalidTopK => "InvalidTopK",
            Self::EmptyQuery => "EmptyQuery",
            Self::Backend(_) => "Backend",
            Self::Store(_) => "Store",
            Self::Index(_) => "Index",
            Self::Embedder(_) => "Embedder",
        }
    }
}

impl IntoResponse for SearchError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_name = self.error_name();
        let body = ErrorBody {
            error: error_name,
            message: self.to_string(),
        };

        (status, Json(body)).into_response()
    }
}
