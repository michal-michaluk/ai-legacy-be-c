//! Per-context auth error enum with stable string codes.
//!
//! `AuthError` maps into the generic `ApiError` (in `rest.rs`). JWT failures
//! are the act bucket (401); revoked is 401; rate-limit is the retry bucket.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    /// `Authorization` header missing or malformed.
    #[error("auth:missing_credentials")]
    MissingCredentials,
    /// Signature / malformed token.
    #[error("auth:invalid_token")]
    InvalidToken,
    /// Token expired (A14).
    #[error("auth:expired")]
    Expired,
    /// Wrong issuer.
    #[error("auth:invalid_issuer")]
    InvalidIssuer,
    /// Wrong audience — not for this service (A15).
    #[error("auth:invalid_audience")]
    InvalidAudience,
    /// `jti` has been revoked — logout (A14).
    #[error("auth:revoked")]
    Revoked,
    /// Throttled auth endpoint (A9).
    #[error("auth:rate_limited")]
    RateLimited,
    /// IdP / JWKS unreachable or unexpected — transient.
    #[error("auth:unavailable")]
    Unavailable,
    /// Unexpected internal error — opaque (A10).
    #[error("auth:internal")]
    Internal,
}

impl AuthError {
    /// Retry bucket: rate-limited and unavailable are transient.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Unavailable)
    }

    /// Bug bucket: never echo internal detail (A10).
    pub fn opaque(&self) -> bool {
        matches!(self, Self::Internal)
    }
}
