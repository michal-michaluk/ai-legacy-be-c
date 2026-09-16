//! `settings` slice — the tenant-scoped JSON config store bounded context.
//!
//! Re-exports only its ports + router; model internals stay private. Cross-slice
//! access (the `auth` kernel types) is via an injected `Authority` argument.

mod error;
mod model;
mod pg_settings;
mod rest;
mod settings;
mod settings_port;
mod settings_repository;

pub use error::SettingsError;
pub use pg_settings::millis_to_instant;
pub use pg_settings::PgSettingsRepository;
pub use rest::{settings_routes, SettingResponse};
pub use settings::SettingsService;
pub use settings_port::SettingsServicePort;
pub use settings_repository::{SearchPage, SettingRepository};
// Public domain model surface (contracts.rs + cross-module tests reference the
// aggregate / value objects; the model stays serde-free and framework-free).
pub use model::{Setting, SettingKey, SettingSnapshot, SettingValue};
