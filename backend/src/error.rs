//! Unified error type: every handler returns `AppResult<T>`; nothing panics at runtime.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    /// 401 with a machine-readable `code` (e.g. `telegram_required`).
    #[error("{0}")]
    Unauthorized(String, &'static str),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>, code: &'static str) -> Self {
        Self::Unauthorized(msg.into(), code)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg, code) = match &self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone(), "bad_request"),
            AppError::Unauthorized(m, code) => (StatusCode::UNAUTHORIZED, m.clone(), *code),
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"), "internal")
            }
        };
        (status, Json(json!({ "error": msg, "code": code }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
