//! The `settings` slice's secondary port — the persistence contract.
//!
//! Native async (no `#[async_trait]`, no trait objects, no `futures` crate):
//! each method is declared with **return-position `impl Future + Send`**, so the
//! future's `Send` guarantee is part of the trait contract under generic static
//! dispatch — which is what lets `axum` handlers await them. Impls stay `async
//! fn`. This trait knows nothing about `sqlx`, `axum`, or any IO crate.
//! `Pagination` is passed **by reference** so the adapter computes
//! `LIMIT/OFFSET`.

use std::future::Future;

use crate::auth::{Authority, Ownership, Tenant};
use crate::common::pagination::Pagination;
use crate::settings::error::SettingsError;
use crate::settings::model::{Setting, SettingKey};

/// One page of a fuzzy search plus the **total** of the SAME filtered set the
/// page came from. Both are computed by a single CTE (the `visible` set is
/// shared, so `LIMIT/OFFSET` and `COUNT(*)` never disagree) — the total is NOT
/// a whole-tenant count (D1). `rows` are the current page; `total` is the count
/// of all rows matching the WHERE filter.
#[derive(Debug, Clone)]
pub struct SearchPage {
    pub rows: Vec<Setting>,
    pub total: u64,
}

/// The persistence port for the `settings` aggregate. Generic static dispatch.
pub trait SettingRepository: Send + Sync {
    /// All rows for `(tenant, key)` — the authority's scope is resolved by the
    /// service, so one logical key may have multiple `Ownership` rows.
    fn find(
        &self,
        key: &SettingKey,
        tenant: &Tenant,
    ) -> impl Future<Output = Result<Vec<Setting>, SettingsError>> + Send;

    /// Create a new aggregate. Duplicate `(tenant, user, key)` -> `Conflict`.
    fn insert(
        &self,
        setting: &Setting,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send;

    /// **Conditional atomic** update (be-arch §10): succeeds only when the row
    /// still has `version == expected`. `expected` is the client's echoed
    /// `If-Match` precondition, never a client-authorised new version. Stale ->
    /// `VersionConflict`; absent -> `NotFound`.
    fn update(
        &self,
        setting: &Setting,
        expected_version: u64,
    ) -> impl Future<Output = Result<Setting, SettingsError>> + Send;

    /// **Conditional atomic** delete (be-arch §10). Stale -> `VersionConflict`;
    /// absent -> `NotFound`.
    fn delete(
        &self,
        key: &SettingKey,
        ownership: &Ownership,
        expected_version: u64,
    ) -> impl Future<Output = Result<(), SettingsError>> + Send;

    /// Liveness of the backing store for the `/readyz` readiness probe: a
    /// round-trip `SELECT 1`. Err means the DB is not reachable -> NOT ready.
    fn ping(&self) -> impl Future<Output = Result<(), SettingsError>> + Send;

    /// Fuzzy search over `key` + `value` (`jsonb::text`) with real pagination.
    ///
    /// The read-permission predicate is pushed into the SQL `WHERE` so the
    /// page (`LIMIT/OFFSET`) counts **only the rows the authority may read** —
    /// a caller's page is never misaligned by rows it cannot see (D1). The
    /// repository builds the ownership predicate from `authority`. Returns the
    /// page rows plus the `total` of the SAME visible (filtered) set, from one
    /// SQL statement.
    fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> impl Future<Output = Result<SearchPage, SettingsError>> + Send;
}
