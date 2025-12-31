//! Error types for ram-api.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

/// Result type for ram-api operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ram-api.
#[derive(Debug, Error)]
pub enum Error {
    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Invalid request.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Unauthorized access.
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden - authenticated but not authorized.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Service unavailable - remote node unreachable.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Core error.
    #[error("core error: {0}")]
    Core(#[from] ram_core::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::Core(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        let body = Json(json!({
            "error": message,
            "code": status.as_u16()
        }));

        (status, body).into_response()
    }
}
