use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Embedding API error: {0}")]
    Embedding(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[allow(dead_code)]
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[allow(dead_code)]
    #[error("Validation error: {0}")]
    Validation(String),

    #[allow(dead_code)]
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for DoryError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            DoryError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            DoryError::Embedding(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            DoryError::SecurityViolation(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            DoryError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            DoryError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            DoryError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            DoryError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}

pub type DoryResult<T> = Result<T, DoryError>;