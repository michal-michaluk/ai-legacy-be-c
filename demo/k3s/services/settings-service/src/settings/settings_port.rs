//! The `settings` slice's **primary port** — the service contract the REST
//! adapter depends on.
//!
//! Native async (no `#[async_trait]`, no trait objects): each method is declared
//! with **return-position `impl Future + Send`**, so the future's `Send`
//! guarantee is part of the trait contract under generic static dispatch — the
//! same convention as `SettingRepository`. This lets an `axum` handler hold the
//! port (not the concrete `SettingsService`) in `AppState`, which is what makes
//! the service injectable at the HTTP-adapter boundary (variant B: the pact
//! harness mocks here, not at the repository seam).
//!
//! `SettingsService<R>` implements this port by delegation, so production code
//! keeps compiling unchanged and still satisfies the port.

use std::future::Future;
use std::time::Instant;

use crate::auth::Authority;
use crate::common::pagination::Pagination;
use crate::settings::error::SettingsError;
use crate::settings::model::{Setting, SettingKey, SettingValue};
use crate::settings::settings::SettingsService;
use crate::settings::settings_repository::SearchPage;
use crate::settings::settings_repository::SettingRepository;

/// The primary port for the `settings` aggregate. Generic static dispatch.
pub trait SettingsServicePort: Send + Sync {
    fn get(
        &self,
        key: &SettingKey,
        authority: &Authority,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send;

    fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> impl Future<Output = Result<SearchPage, SettingsError>> + Send;

    fn put(
        &self,
        key: SettingKey,
        value: SettingValue,
        authority: &Authority,
        expected: Option<u64>,
        now: Instant,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send;

    fn patch(
        &self,
        key: SettingKey,
        patch: SettingValue,
        authority: &Authority,
        expected: u64,
        now: Instant,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send;

    fn delete(
        &self,
        key: SettingKey,
        authority: &Authority,
        expected: u64,
    ) -> impl Future<Output = Result<(), SettingsError>> + Send;

    /// Readiness of the service (backing store reachable) for `/readyz`.
    fn ready(&self) -> impl Future<Output = Result<(), SettingsError>> + Send;
}

impl<R: SettingRepository> SettingsServicePort for SettingsService<R> {
    fn get(
        &self,
        key: &SettingKey,
        authority: &Authority,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send {
        SettingsService::get(self, key, authority)
    }

    fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> impl Future<Output = Result<SearchPage, SettingsError>> + Send {
        SettingsService::search(self, query, authority, pagination)
    }

    fn put(
        &self,
        key: SettingKey,
        value: SettingValue,
        authority: &Authority,
        expected: Option<u64>,
        now: Instant,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send {
        SettingsService::put(self, key, value, authority, expected, now)
    }

    fn patch(
        &self,
        key: SettingKey,
        patch: SettingValue,
        authority: &Authority,
        expected: u64,
        now: Instant,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send {
        SettingsService::patch(self, key, patch, authority, expected, now)
    }

    fn delete(
        &self,
        key: SettingKey,
        authority: &Authority,
        expected: u64,
    ) -> impl Future<Output = Result<(), SettingsError>> + Send {
        SettingsService::delete(self, key, authority, expected)
    }

    fn ready(&self) -> impl Future<Output = Result<(), SettingsError>> + Send {
        SettingsService::ready(self)
    }
}
