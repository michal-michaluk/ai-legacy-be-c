//! Per-context settings error enum with stable string codes.
//!
//! Codes are the contract — the client switches on them, never on prose. The
//! `code-structure` NOGO forbids `sqlx` in this slice's core files, so
//! persistence / transient classification happens in the adapter
//! (`pg_settings.rs`), which maps sqlx errors into `Internal` (opaque 500) or
//! `Unavailable` (transient 503).

use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum SettingsError {
    /// Resource not found (act bucket, 404).
    #[error("settings:not_found")]
    NotFound,
    /// Key failed value-object validation (act bucket, 400).
    #[error("settings:invalid_key")]
    InvalidKey,
    /// Value failed value-object validation (not an object / oversized) (act, 422).
    #[error("settings:invalid_value")]
    InvalidValue,
    /// Value state conflict, e.g. duplicate create (act bucket, 409).
    #[error("settings:value_conflict")]
    Conflict,
    /// Stale `If-Match` precondition — optimistic concurrency (act bucket, 409).
    #[error("settings:version_conflict")]
    VersionConflict,
    /// Not authorized for this resource (act bucket, 403).
    #[error("settings:forbidden")]
    Forbidden,
    /// Throttled (retry bucket, 429).
    #[error("settings:rate_limited")]
    RateLimited,
    /// Transient backend hiccup (retry bucket, 503).
    #[error("settings:unavailable")]
    Unavailable,
    /// Unexpected internal error — opaque (bug bucket, 500, code `settings:internal`).
    #[error("settings:internal")]
    Internal,
}

impl SettingsError {
    /// Retry bucket: rate-limited and unavailable are transient.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Unavailable)
    }

    /// Bug bucket: never echo internal detail (A10).
    pub fn opaque(&self) -> bool {
        matches!(self, Self::Internal)
    }
}
