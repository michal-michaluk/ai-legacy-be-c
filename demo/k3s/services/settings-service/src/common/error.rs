//! Generic presentation error mapper (`ApiError`) — imports NO context.
//!
//! Each slice owns its domain error; this module maps any slice error into one
//! generic HTTP error whose stable `code` is the contract. This is the single
//! error boundary: no stack trace, SQL or internal detail ever reaches a
//! response body (security A10).

use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::common::pagination::PaginationError;

/// The JSON body carried over the wire.
#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

/// One error type crosses the boundary (the single error boundary). Buckets
/// (be-arch §9): act -> 400/401/403/404/409/422 (verbatim message); retry ->
/// 429/503 + `Retry-After` (generic); bug -> 500 (opaque, A10).
#[derive(Debug, Clone)]
pub struct ApiError {
    pub code: &'static str,
    pub status: StatusCode,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl ApiError {
    /// Act bucket: client changes something and re-sends. Message shown verbatim.
    pub fn act(code: &'static str, status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            code,
            status,
            message: message.into(),
            retry_after: None,
        }
    }

    /// Retry bucket (429/503): generic message, with `Retry-After`.
    pub fn retry(code: &'static str, status: StatusCode, retry_after: u64) -> Self {
        Self {
            code,
            status,
            message: "Retry later".to_string(),
            retry_after: Some(retry_after),
        }
    }

    /// Bug bucket (500): log full detail, return an opaque generic body (A10).
    pub fn bug(code: &'static str, detail: impl std::fmt::Display) -> Self {
        tracing::error!(error = %detail, code = %code, "internal error");
        Self {
            code,
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal error".to_string(),
            retry_after: None,
        }
    }

    /// Shortcut for the common missing-credentials 401 (act bucket).
    pub fn unauthorized() -> Self {
        Self::act(
            "auth:missing_credentials",
            StatusCode::UNAUTHORIZED,
            "Authentication required",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut builder = Response::builder()
            .status(self.status)
            .header("X-Error-Code", self.code);
        if let Some(seconds) = self.retry_after {
            builder = builder.header(header::RETRY_AFTER, seconds.to_string());
        }
        let body = ApiErrorBody {
            code: self.code.to_string(),
            message: self.message,
            retry_after: self.retry_after,
        };
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }
}

/// `PaginationError` maps to a **400** (act bucket) — malformed query params are
/// a client error, never a 500.
impl From<PaginationError> for ApiError {
    fn from(e: PaginationError) -> Self {
        match e {
            PaginationError::BadPage => Self::act(
                "pagination:bad_page",
                StatusCode::BAD_REQUEST,
                "'page' must be >= 1",
            ),
            PaginationError::BadSize => Self::act(
                "pagination:bad_size",
                StatusCode::BAD_REQUEST,
                "'size' must be between 1 and 100",
            ),
        }
    }
}
