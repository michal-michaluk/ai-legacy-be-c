//! `SettingsService` — the settings slice's primary port implementation, generic
//! over its **single** secondary port `SettingRepository` (ADR-003: one generic
//! param, not a bundle). Business rules and ownership checks live here; the
//! domain model enforces invariants via its value objects / aggregate.

use std::time::Instant;

use tracing::instrument;

use crate::auth::Authority;
use crate::common::pagination::Pagination;
use crate::settings::error::SettingsError;
use crate::settings::model::{Setting, SettingKey, SettingValue};
use crate::settings::settings_repository::{SearchPage, SettingRepository};

/// Generic over `R` — the injected repository adapter (static dispatch).
pub struct SettingsService<R: SettingRepository> {
    repo: R,
}

impl<R: SettingRepository> SettingsService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// Read one setting. A resource that exists in the tenant but is not
    /// readable by this authority -> `Forbidden`.
    #[instrument(skip(self, authority), fields(tenant = %authority.tenant(), user = %authority.user(), role = %authority.role()))]
    pub async fn get(
        &self,
        key: &SettingKey,
        authority: &Authority,
    ) -> Result<Setting, SettingsError> {
        let rows = self.repo.find(key, authority.tenant()).await?;
        if rows.is_empty() {
            return Err(SettingsError::NotFound);
        }
        rows.into_iter()
            .find(|s| authority.can_read(s.ownership()))
            .ok_or(SettingsError::Forbidden)
    }

    /// Fuzzy search on key + value with real pagination, restricted to what the
    /// authority can read (no cross-user leakage, D1). The read-permission
    /// predicate is pushed into the repository SQL, so `LIMIT/OFFSET` counts
    /// only rows visible to the authority and the in-memory filter is omitted
    /// (a caller's page is never truncated/misaligned by unreadable rows).
    #[instrument(skip(self, authority), fields(tenant = %authority.tenant(), user = %authority.user(), role = %authority.role()))]
    pub async fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> Result<SearchPage, SettingsError> {
        self.repo.search(query, authority, pagination).await
    }

    /// PUT: create or replace the whole value. Replacing an existing resource
    /// requires an `If-Match` precondition (`expected`) — a mismatch is a
    /// `VersionConflict` (409), enforced atomically in the adapter.
    #[instrument(skip(self, authority, value), fields(tenant = %authority.tenant(), user = %authority.user(), role = %authority.role()))]
    pub async fn put(
        &self,
        key: SettingKey,
        value: SettingValue,
        authority: &Authority,
        expected: Option<u64>,
        now: Instant,
    ) -> Result<Setting, SettingsError> {
        let target = authority.scope_ownership();
        let rows = self.repo.find(&key, authority.tenant()).await?;
        if let Some(cur) = rows.iter().find(|s| s.ownership() == &target) {
            let expected = expected.ok_or(SettingsError::VersionConflict)?;
            if expected != cur.version() {
                return Err(SettingsError::VersionConflict);
            }
            let mut next = cur.clone();
            next.set_value(value, now)?;
            self.repo.update(&next, expected).await
        } else {
            let setting = Setting::new(key, value, target, now)?;
            self.repo.insert(&setting).await
        }
    }

    /// PATCH: partial (JSON merge) into the existing value. Requires an
    /// `If-Match` precondition; stale -> `VersionConflict`.
    #[instrument(skip(self, authority, patch), fields(tenant = %authority.tenant(), user = %authority.user(), role = %authority.role()))]
    pub async fn patch(
        &self,
        key: SettingKey,
        patch: SettingValue,
        authority: &Authority,
        expected: u64,
        now: Instant,
    ) -> Result<Setting, SettingsError> {
        let target = authority.scope_ownership();
        let rows = self.repo.find(&key, authority.tenant()).await?;
        if rows.is_empty() {
            return Err(SettingsError::NotFound);
        }
        let cur = rows
            .iter()
            .find(|s| s.ownership() == &target)
            .ok_or(SettingsError::Forbidden)?;
        if expected != cur.version() {
            return Err(SettingsError::VersionConflict);
        }
        let mut next = cur.clone();
        next.apply_partial(patch, now)?;
        self.repo.update(&next, expected).await
    }

    /// DELETE: conditional on `If-Match`. Stale -> `VersionConflict`.
    #[instrument(skip(self, authority), fields(tenant = %authority.tenant(), user = %authority.user(), role = %authority.role()))]
    pub async fn delete(
        &self,
        key: SettingKey,
        authority: &Authority,
        expected: u64,
    ) -> Result<(), SettingsError> {
        let target = authority.scope_ownership();
        let rows = self.repo.find(&key, authority.tenant()).await?;
        if rows.is_empty() {
            return Err(SettingsError::NotFound);
        }
        let cur = rows
            .iter()
            .find(|s| s.ownership() == &target)
            .ok_or(SettingsError::Forbidden)?;
        if expected != cur.version() {
            return Err(SettingsError::VersionConflict);
        }
        self.repo
            .delete(&key, &cur.ownership().clone(), expected)
            .await
    }

    /// Readiness probe: forwards to the repository's `ping`. `Ok` means the
    /// backing store is reachable. Used by the `/readyz` route.
    #[instrument(skip(self), fields(service = "settings-service"))]
    pub async fn ready(&self) -> Result<(), SettingsError> {
        self.repo.ping().await
    }
}

// ---------------------------------------------------------------------------
// Real-repo (sqlx) integration tests. These run against a disposable test
// Postgres created by `#[sqlx::test]` (migrations applied per-test), NOT an
// in-memory repository double — so the persistence + optimistic-locking
// behaviour (be-arch §10) is proven on the real adapter.
//
// Gated behind the `pg-integration` feature so the DEFAULT `cargo test --lib`
// is DB-less and never requires `DATABASE_URL` (see also the `pg_support`
// module, reached through `src/lib.rs`, which starts the testcontainer and
// exports `DATABASE_URL` when the feature is enabled).
// ---------------------------------------------------------------------------
#[cfg(test)]
#[cfg(feature = "pg-integration")]
mod integration {
    use super::super::pg_settings::PgSettingsRepository;
    use super::*;
    use crate::auth::{Authority, InMemoryRevocationStore, JwtAuthAdapter, Role, Tenant, UserId};
    use crate::common::rate_limit::RateLimiter;
    use crate::settings::rest::settings_routes;
    use crate::state::AppState;
    use tower::ServiceExt;

    fn now() -> Instant {
        Instant::now()
    }
    fn tenant() -> Tenant {
        Tenant::new("acme")
    }
    fn user(id: &str) -> UserId {
        UserId::new(id)
    }
    fn alice() -> Authority {
        Authority::new(tenant(), user("alice"), Role::User)
    }
    fn admin() -> Authority {
        Authority::new(tenant(), user("root"), Role::Admin)
    }
    fn value(v: serde_json::Value) -> SettingValue {
        SettingValue::new(v).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn user_cannot_read_another_users_setting(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        let bob = Authority::new(tenant(), user("bob"), Role::User);
        let err = svc
            .get(&SettingKey::new("theme").unwrap(), &bob)
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::Forbidden));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_can_read_tenant_scoped_setting(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            &admin(),
            None,
            now(),
        )
        .await
        .unwrap();
        let s = svc
            .get(&SettingKey::new("theme").unwrap(), &admin())
            .await
            .unwrap();
        assert_eq!(
            s.value().as_value(),
            &serde_json::json!({ "color": "dark" })
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stale_precondition_conflicts_and_leaves_resource_unchanged(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        let key = SettingKey::new("theme").unwrap();
        svc.put(
            key.clone(),
            value(serde_json::json!({ "color": "dark" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        // A successful write bumps the server version to 2.
        svc.put(
            key.clone(),
            value(serde_json::json!({ "color": "midnight" })),
            &alice(),
            Some(1),
            now(),
        )
        .await
        .unwrap();
        // Now a stale precondition (version 1) must fail and leave the value at "midnight".
        let err = svc
            .put(
                key.clone(),
                value(serde_json::json!({ "color": "light" })),
                &alice(),
                Some(1),
                now(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::VersionConflict));
        let s = svc.get(&key, &alice()).await.unwrap();
        assert_eq!(s.version(), 2);
        assert_eq!(
            s.value().as_value(),
            &serde_json::json!({ "color": "midnight" })
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_writers_exactly_one_wins(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        let key = SettingKey::new("theme").unwrap();
        svc.put(
            key.clone(),
            value(serde_json::json!({ "v": 0 })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        let a = svc
            .put(
                key.clone(),
                value(serde_json::json!({ "a": 1 })),
                &alice(),
                Some(1),
                now(),
            )
            .await;
        let b = svc
            .put(
                key.clone(),
                value(serde_json::json!({ "b": 2 })),
                &alice(),
                Some(1),
                now(),
            )
            .await;
        let wins = [a.is_ok(), b.is_ok()].iter().filter(|w| **w).count();
        let conflicts = [&a, &b]
            .iter()
            .filter(|r| matches!(r, Err(SettingsError::VersionConflict)))
            .count();
        assert_eq!(wins, 1);
        assert_eq!(conflicts, 1);
    }

    // -------------------------------------------------------------------
    // REST handler tests via tower::oneshot (no socket) against the real repo.
    // -------------------------------------------------------------------
    fn app(pool: sqlx::PgPool) -> axum::Router {
        let settings = SettingsService::new(PgSettingsRepository::new(pool));
        let auth = crate::auth::AuthService::new(
            JwtAuthAdapter::new(
                "http://keycloak/realm".into(),
                "settings-service".into(),
                "http://keycloak/certs".into(),
            ),
            InMemoryRevocationStore::new(),
        );
        let state = AppState::new(
            settings,
            auth,
            RateLimiter::new(100_000, 200_000, std::time::Duration::from_secs(60)),
            "http://localhost:4317".into(),
            prometheus::Registry::new(),
            reqwest::Client::new(),
        );
        settings_routes::<
            JwtAuthAdapter,
            InMemoryRevocationStore,
            SettingsService<PgSettingsRepository>,
        >()
        .with_state(state)
    }

    async fn request(
        app: &axum::Router,
        method: &str,
        uri: &str,
        authority: Authority,
    ) -> axum::http::Response<axum::body::Body> {
        let mut req = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::empty())
            .unwrap();
        req.extensions_mut().insert(authority);
        app.clone().oneshot(req).await.unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rest_get_returns_etag_and_put_requires_if_match(pool: sqlx::PgPool) {
        // Seed via the service, then GET over HTTP returns ETag "1".
        let svc = SettingsService::new(PgSettingsRepository::new(pool.clone()));
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();

        // Sanity: the row IS in the DB (service.get finds it).
        let direct = svc.get(&SettingKey::new("theme").unwrap(), &alice()).await;
        assert!(
            direct.is_ok(),
            "service.get must find the seeded row: {direct:?}"
        );

        let app = app(pool);
        let resp = request(&app, "GET", "/theme", alice()).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get(axum::http::header::ETAG).unwrap(),
            "\"1\""
        );

        // A stale If-Match precondition returns 409 settings:version_conflict.
        let stale = axum::http::Request::builder()
            .method("PUT")
            .uri("/theme")
            .header(axum::http::header::IF_MATCH, "\"99\"")
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({ "value": { "color": "light" } }).to_string(),
            ))
            .unwrap();
        let mut stale = stale;
        stale.extensions_mut().insert(alice());
        let resp = app.clone().oneshot(stale).await.unwrap();
        assert_eq!(resp.status(), 409);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("settings:version_conflict"));
        assert!(
            !text.contains("sqlx") && !text.contains("stack"),
            "no internal detail leaked (A10)"
        );
    }

    // -------------------------------------------------------------------
    // pg_settings.rs full-coverage: `delete` (success/NotFound/VersionConflict),
    // `search` (filtered/paginated/empty), `exists` (update-missing path),
    // `from_url`, and the tenant-scoped `into_domain` branch.
    // -------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_removes_row_and_frees_key(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        let key = SettingKey::new("theme").unwrap();
        svc.put(
            key.clone(),
            value(serde_json::json!({ "color": "dark" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        // Successful conditional delete (version 1).
        svc.delete(key.clone(), &alice(), 1).await.unwrap();
        // Gone: get -> NotFound (empty rows at service level).
        assert!(matches!(
            svc.get(&key, &alice()).await.unwrap_err(),
            SettingsError::NotFound
        ));
        // Key is recreatable (insert path, no Conflict).
        svc.put(
            key,
            value(serde_json::json!({ "color": "light" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_missing_key_not_found(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        let err = svc
            .delete(SettingKey::new("nope").unwrap(), &alice(), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::NotFound));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_stale_precondition_conflicts(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        let key = SettingKey::new("theme").unwrap();
        svc.put(
            key.clone(),
            value(serde_json::json!({ "v": 1 })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        svc.put(
            key.clone(),
            value(serde_json::json!({ "v": 2 })),
            &alice(),
            Some(1),
            now(),
        )
        .await
        .unwrap();
        let err = svc.delete(key.clone(), &alice(), 1).await.unwrap_err();
        assert!(matches!(err, SettingsError::VersionConflict));
        assert_eq!(svc.get(&key, &alice()).await.unwrap().version(), 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn search_filters_and_paginates_by_authority(pool: sqlx::PgPool) {
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "midnight" })),
            &admin(),
            None,
            now(),
        )
        .await
        .unwrap();
        svc.put(
            SettingKey::new("locale").unwrap(),
            value(serde_json::json!({ "lang": "pl" })),
            &alice(),
            None,
            now(),
        )
        .await
        .unwrap();
        // alice sees only her own "theme" row (D1 no cross-user leak).
        let first = svc
            .search("theme", &alice(), &Pagination::default_page())
            .await
            .unwrap();
        assert_eq!(first.total, 1);
        // size 1 -> one row, total still the visible-set count.
        let page = Pagination::new(1, 1).unwrap();
        assert_eq!(
            svc.search("theme", &alice(), &page)
                .await
                .unwrap()
                .rows
                .len(),
            1
        );
        // No matches -> empty rows, total 0.
        let empty = svc.search("no-match", &alice(), &page).await.unwrap();
        assert_eq!(empty.rows.len(), 0);
        assert_eq!(empty.total, 0);
        // Out-of-range page: LEFT JOIN sentinel, no panic, rows still 0.
        let far = svc
            .search("theme", &alice(), &Pagination::new(999, 1).unwrap())
            .await
            .unwrap();
        assert_eq!(far.rows.len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn update_missing_row_not_found_via_exists(pool: sqlx::PgPool) {
        let repo = PgSettingsRepository::new(pool);
        let s = Setting::new(
            SettingKey::new("ghost").unwrap(),
            value(serde_json::json!({ "x": 1 })),
            alice().scope_ownership(),
            now(),
        )
        .unwrap();
        // update on absent key -> exists=false -> NotFound (covers `exists`).
        assert!(matches!(
            repo.update(&s, 1).await.unwrap_err(),
            SettingsError::NotFound
        ));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_duplicate_conflict(pool: sqlx::PgPool) {
        let repo = PgSettingsRepository::new(pool);
        let s = Setting::new(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            alice().scope_ownership(),
            now(),
        )
        .unwrap();
        repo.insert(&s).await.unwrap();
        // Duplicate (tenant,user,key) -> ON CONFLICT DO NOTHING -> Conflict.
        assert!(matches!(
            repo.insert(&s).await.unwrap_err(),
            SettingsError::Conflict
        ));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn tenant_scoped_row_reads_back_into_domain(pool: sqlx::PgPool) {
        // Admin inserts a tenant-scoped row (user NULL); search as admin sees it,
        // exercising into_domain's Ownership::for_tenant branch.
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        svc.put(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "color": "dark" })),
            &admin(),
            None,
            now(),
        )
        .await
        .unwrap();
        let page = Pagination::default_page();
        let res = svc.search("theme", &admin(), &page).await.unwrap();
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].ownership().user().map(|u| u.as_str()), None);
    }

    #[test]
    fn from_url_rejects_malformed_url() {
        // `PgPoolOptions::connect_lazy` parses the URL synchronously, so a
        // malformed URL errors through map_pg_err.
        assert!(PgSettingsRepository::from_url("not a url ://").is_err());
    }

    #[tokio::test]
    async fn from_url_accepts_reachable_dsn() {
        // connect_lazy builds the pool from a REAL registered DSN without
        // connecting (the testcontainer exposes DATABASE_URL). Requires a Tokio
        // runtime because the pool spawns a background maintenance task. This is
        // the only way to reach the Ok(Self{pool}) path (line 89); a fabricated
        // hostname panics in sqlx's connect_lazy because it resolves the host.
        let dsn = std::env::var("DATABASE_URL").expect("testcontainer DATABASE_URL");
        assert!(PgSettingsRepository::from_url(&dsn).is_ok());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn repo_update_stale_version_conflicts_via_exists(pool: sqlx::PgPool) {
        // Bypass the service short-circuit: call the repo directly with a stale
        // expected_version, so `update` finds no matching row but `exists` is
        // true -> VersionConflict (covers pg_settings.rs line 165).
        let repo = PgSettingsRepository::new(pool);
        let s = Setting::new(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "v": 1 })),
            alice().scope_ownership(),
            now(),
        )
        .unwrap();
        repo.insert(&s).await.unwrap();
        // Same key now exists (version 1); attempt update with expected 99 -> no
        // row matches the version predicate, but the key exists -> Conflict.
        let err = repo.update(&s, 99).await.unwrap_err();
        assert!(matches!(err, SettingsError::VersionConflict));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn repo_delete_stale_version_conflicts(pool: sqlx::PgPool) {
        // Direct repo call: `delete` with a stale version hits exists=true ->
        // VersionConflict (line 195), and the row must survive (line 197 guard).
        let repo = PgSettingsRepository::new(pool);
        let s = Setting::new(
            SettingKey::new("theme").unwrap(),
            value(serde_json::json!({ "v": 1 })),
            alice().scope_ownership(),
            now(),
        )
        .unwrap();
        repo.insert(&s).await.unwrap();
        let err = repo
            .delete(s.key(), &alice().scope_ownership(), 99)
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::VersionConflict));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn repo_delete_absent_versions_not_found(pool: sqlx::PgPool) {
        // Direct repo call with NO matching row at all -> exists=false ->
        // NotFound (line 194-195 else branch).
        let repo = PgSettingsRepository::new(pool);
        let err = repo
            .delete(
                &SettingKey::new("ghost").unwrap(),
                &alice().scope_ownership(),
                1,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::NotFound));
    }

    #[tokio::test]
    async fn repo_find_reports_unavailable_on_pool_timeout() {
        // Force map_pg_err's `PoolTimedOut` arm (lines 285-286 -> Unavailable):
        // a pool to a blackhole address with a 1ms acquire timeout can never
        // hand out a connection, so find() times out. `connect_lazy` + no
        // connection attempt means the bad host only fails at acquire time.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_millis(1))
            .connect_lazy("postgres://u:p@10.255.255.1:5432/db")
            .unwrap();
        let repo = PgSettingsRepository::new(pool);
        let err = repo
            .find(&SettingKey::new("theme").unwrap(), &tenant())
            .await
            .unwrap_err();
        assert!(matches!(err, SettingsError::Unavailable));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn repo_ping_ok_with_reachable_store(pool: sqlx::PgPool) {
        // `ping` round-trips `SELECT 1::BIGINT`; on a live testcontainer it's Ok.
        let repo = PgSettingsRepository::new(pool);
        repo.ping().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn readyz_returns_ok_when_store_reachable(pool: sqlx::PgPool) {
        // Readiness probe via the service's `ready` (delegates to repo.ping).
        let svc = SettingsService::new(PgSettingsRepository::new(pool));
        svc.ready().await.unwrap();
    }
}
