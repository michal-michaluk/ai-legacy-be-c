//! In-process Pact **provider** verification of the REST adapter — no Postgres,
//! no Keycloak/JWKS, no deployment.
//!
//! The real `axum` router (`settings_service::router::build_router` — the exact
//! same router as the composition root, no duplicate) is built against **port
//! doubles for the primary/secondary ports**:
//!   * `StubSettingsService` — a `SettingsServicePort` double (the settings
//!     **primary** port) holding a small in-memory store, seeded per
//!     interaction via the Pact provider-state callback. This is variant B: the
//!     service logic is mocked at the primary port, NOT at the repository seam,
//!     so the harness exercises only the REST adapter (routes, `require_auth`,
//!     DTO serde, `SettingKey`/`SettingValue` validation,
//!     `From<SettingsError> for ApiError`, security headers);
//!   * `PactJwtValidator` — a `JwtValidator` double that accepts the Pact's
//!     (unsigned) bearer token and maps its `tenant`/`role` claims to an
//!     `Authority` (the IdP port);
//!   * the crate's `InMemoryRevocationStore` (the logout port).
//!
//! The router is served on an ephemeral port inside this test process, and
//! `pact_verifier` verifies `pact/settings-service.json` against it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pact_models::provider_states::ProviderState;
use pact_verifier::callback_executors::ProviderStateExecutor;
use pact_verifier::{
    verify_provider_async, FilterInfo, NullRequestFilterExecutor, PactSource, ProviderInfo,
    ProviderTransport, VerificationOptions,
};
use settings_service::auth::{
    AuthError, AuthService, Authority, InMemoryRevocationStore, JwtPrincipal, JwtValidator,
    Ownership, Role, Tenant, UserId,
};
use settings_service::common::config::Config;
use settings_service::common::pagination::Pagination;
use settings_service::common::rate_limit::RateLimiter;
use settings_service::router::build_router;
use settings_service::settings::{
    SearchPage, Setting, SettingKey, SettingValue, SettingsError, SettingsServicePort,
};
use settings_service::state::AppState;

/// Fixed epoch-millis for a deterministic `updated_at`. The anchored
/// `millis_to_instant` base captures the wall-clock `now` once per process; to
/// guarantee the round-trip yields EXACTLY these millis, the value must be in
/// the FUTURE of that base (a past epoch clamps to the base and would drift per
/// run). 4102444800123 ms is `2100-01-01T00:00:00.123Z`.
const FIXED_MS: i64 = 4_102_444_800_123;

/// One deterministic `Instant` for every seeded/mutated row — the same across
/// all runs, so the RFC-3339 `updated_at` string in every 200 body is stable.
fn fixed_now() -> Instant {
    settings_service::settings::millis_to_instant(FIXED_MS)
}

// ---------------------------------------------------------------------------
// Port double #1 — settings PRIMARY port double (SettingsServicePort).
// ---------------------------------------------------------------------------

/// A `SettingsServicePort` double holding a small in-memory store. It mimics
/// the real service's observable behaviour (ownership access checks, optimistic
/// concurrency) so the REAL handler's `SettingResponse::from` / ETag / error
/// taxonomy produce contract-matching responses. The aggregate is built with
/// real constructors (`Setting::new` / `Setting::restore`).
#[derive(Clone)]
struct StubSettingsService {
    rows: Arc<RwLock<Vec<Setting>>>,
    /// When set, `put` reports a duplicate-insert `Conflict` — simulates the real
    /// `PgSettingsRepository::insert` rejecting an existing `(tenant,user,key)`.
    conflict_on_insert: Arc<AtomicBool>,
}

impl StubSettingsService {
    fn new() -> Self {
        Self {
            rows: Arc::new(RwLock::new(Vec::new())),
            conflict_on_insert: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Empty the store — called before every interaction so state never leaks.
    fn reset(&self) {
        *self.rows.write().unwrap() = Vec::new();
        self.conflict_on_insert.store(false, Ordering::SeqCst);
    }

    /// Seed a row (via `Setting::restore` to set an explicit version).
    fn seed(&self, setting: Setting) {
        self.rows.write().unwrap().push(setting);
    }
}

impl SettingsServicePort for StubSettingsService {
    async fn get(&self, key: &SettingKey, authority: &Authority) -> Result<Setting, SettingsError> {
        let rows = self.rows.read().unwrap();
        match rows
            .iter()
            .find(|s| s.key() == key && s.ownership().tenant() == authority.tenant())
        {
            Some(s) if authority.can_read(s.ownership()) => Ok(s.clone()),
            Some(_) => Err(SettingsError::Forbidden),
            None => Err(SettingsError::NotFound),
        }
    }

    async fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> Result<SearchPage, SettingsError> {
        let rows = self.rows.read().unwrap();
        let q = query.to_lowercase();
        // Compute the SAME visible (fully filtered) set first, then derive both
        // the page slice AND the total from it — mirroring the real CTE semantics
        // (`total` == count of the filtered set, not the whole tenant).
        let visible: Vec<Setting> = rows
            .iter()
            .filter(|s| s.ownership().tenant() == authority.tenant())
            .filter(|s| authority.can_read(s.ownership()))
            .filter(|s| {
                q.is_empty()
                    || s.key().as_str().to_lowercase().contains(&q)
                    || s.value().as_value().to_string().to_lowercase().contains(&q)
            })
            .cloned()
            .collect();
        let total = visible.len() as u64;
        let page_rows = visible
            .into_iter()
            .skip(pagination.offset() as usize)
            .take(pagination.limit() as usize)
            .collect();
        Ok(SearchPage {
            rows: page_rows,
            total,
        })
    }

    async fn put(
        &self,
        key: SettingKey,
        value: SettingValue,
        authority: &Authority,
        expected: Option<u64>,
        _now: Instant,
    ) -> Result<Setting, SettingsError> {
        // Deterministic timestamp regardless of what the handler passes in.
        let now = fixed_now();
        // Duplicate-insert `settings:value_conflict` (409). Mirrors the real
        // repository's unique-constraint failure on an existing row.
        if self.conflict_on_insert.load(Ordering::SeqCst) {
            return Err(SettingsError::Conflict);
        }
        let target = authority.scope_ownership();
        let mut rows = self.rows.write().unwrap();
        if let Some(idx) = rows
            .iter()
            .position(|s| s.key() == &key && s.ownership() == &target)
        {
            let cur = rows[idx].clone();
            let expected = expected.ok_or(SettingsError::VersionConflict)?;
            if expected != cur.version() {
                return Err(SettingsError::VersionConflict);
            }
            let mut next = cur.clone();
            next.set_value(value, now)?;
            rows[idx] = next.clone();
            Ok(next)
        } else {
            let setting = Setting::new(key, value, target, now)?;
            rows.push(setting.clone());
            Ok(setting)
        }
    }

    async fn patch(
        &self,
        key: SettingKey,
        patch: SettingValue,
        authority: &Authority,
        expected: u64,
        _now: Instant,
    ) -> Result<Setting, SettingsError> {
        // Deterministic timestamp regardless of what the handler passes in.
        let now = fixed_now();
        let target = authority.scope_ownership();
        let mut rows = self.rows.write().unwrap();
        let idx = rows
            .iter()
            .position(|s| s.key() == &key && s.ownership() == &target)
            .ok_or(SettingsError::NotFound)?;
        let cur = rows[idx].clone();
        if !authority.can_read(cur.ownership()) {
            return Err(SettingsError::Forbidden);
        }
        if expected != cur.version() {
            return Err(SettingsError::VersionConflict);
        }
        let mut next = cur.clone();
        next.apply_partial(patch, now)?;
        rows[idx] = next.clone();
        Ok(next)
    }

    async fn delete(
        &self,
        key: SettingKey,
        authority: &Authority,
        expected: u64,
    ) -> Result<(), SettingsError> {
        let target = authority.scope_ownership();
        let mut rows = self.rows.write().unwrap();
        let idx = rows
            .iter()
            .position(|s| s.key() == &key && s.ownership() == &target)
            .ok_or(SettingsError::NotFound)?;
        let cur = rows[idx].clone();
        if !authority.can_read(cur.ownership()) {
            return Err(SettingsError::Forbidden);
        }
        if expected != cur.version() {
            return Err(SettingsError::VersionConflict);
        }
        rows.remove(idx);
        Ok(())
    }

    // In-memory stub is always ready (no backing store).
    async fn ready(&self) -> Result<(), SettingsError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Port double #2 — JWT validation double (JwtValidator). Accepts any token,
// decodes its `tenant`/`role` claims, maps to an `Authority`.
// ---------------------------------------------------------------------------

/// A `JwtValidator` double: no JWKS/Keycloak. It base64-decodes the Pact's
/// bearer token (which carries `{tenant, role}` claims in its payload segment)
/// and resolves `tenant`/`role` into an `Authority`. The contract's tokens are
/// intentionally unsigned/RS256-header-only — this double is exactly the seam
/// that lets the REST adapter be tested without an IdP.
#[derive(Clone)]
struct PactJwtValidator;

impl PactJwtValidator {
    fn new() -> Self {
        Self
    }
}

impl JwtValidator for PactJwtValidator {
    async fn validate(&self, token: &str) -> Result<JwtPrincipal, AuthError> {
        let payload = token.split('.').nth(1).ok_or(AuthError::InvalidToken)?;
        let json = decode_b64url(payload).ok_or(AuthError::InvalidToken)?;
        let v: serde_json::Value =
            serde_json::from_slice(&json).map_err(|_| AuthError::InvalidToken)?;

        let tenant = v
            .get("tenant")
            .and_then(|x| x.as_str())
            .unwrap_or("acme")
            .to_string();
        let role = match v
            .get("role")
            .and_then(|x| x.as_str())
            .unwrap_or("User")
            .to_ascii_lowercase()
            .as_str()
        {
            "admin" | "administrator" => Role::Admin,
            _ => Role::User,
        };
        let user = v
            .get("sub")
            .and_then(|x| x.as_str())
            .unwrap_or("service-user")
            .to_string();
        let jti = v
            .get("jti")
            .and_then(|x| x.as_str())
            .unwrap_or("pact-jti")
            .to_string();

        Ok(JwtPrincipal::new(
            Authority::new(Tenant::new(&tenant), UserId::new(&user), role),
            jti,
            Duration::from_secs(3600),
        ))
    }
}

fn decode_b64url(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('=').as_bytes())
        .ok()
}

// ---------------------------------------------------------------------------
// Provider-state callback — resets/seeds the service stub per interaction.
// ---------------------------------------------------------------------------

/// Seeds `settings` to satisfy each Pact `providerState`. The store is emptied
/// before every interaction (setup or teardown), so each interaction is
/// isolated regardless of execution order.
#[derive(Clone)]
struct ProviderStateSeeding {
    settings: StubSettingsService,
}

impl std::fmt::Debug for ProviderStateSeeding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderStateSeeding").finish()
    }
}

impl ProviderStateSeeding {
    fn seed_for(&self, state: &str) {
        let now = fixed_now();
        let acme = Tenant::new("acme");
        let tenant = Ownership::for_tenant(acme.clone());
        let theme = SettingKey::new("theme").unwrap();

        // Every state is seeded from an empty store.
        self.settings.reset();
        match state {
            // PUT create (empty store): the stub's `put` creates a version-1 aggregate.
            "an authenticated user with write access to the tenant" => {}
            // GET returns version 2 (a previously-upserted, now-version-2 row).
            "a setting exists for the authenticated user" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({"color": "dark"})).unwrap(),
                        tenant,
                        2,
                        now,
                    )
                    .unwrap(),
                );
            }
            // PATCH merges into a version-1 row -> v2 {color:dark, font:mono}.
            "a setting exists with version 1 for the authenticated user" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({"color": "dark"})).unwrap(),
                        tenant,
                        1,
                        now,
                    )
                    .unwrap(),
                );
            }
            // DELETE with If-Match "3" against a version-3 row -> 204.
            "a setting exists with version 3 for the authenticated user" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({"color": "dark"})).unwrap(),
                        tenant,
                        3,
                        now,
                    )
                    .unwrap(),
                );
            }
            // Stale If-Match "1" against a version-2 row -> 409.
            "a setting exists at version 2 for the authenticated user" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({"color": "midnight"})).unwrap(),
                        tenant,
                        2,
                        now,
                    )
                    .unwrap(),
                );
            }
            // GET missing key -> empty store -> 404.
            "the setting does not exist for the authenticated user" => {}
            // Non-admin User vs a tenant-scoped row -> 403.
            "a tenant-scoped setting exists but the user is not an admin" => {
                self.settings.seed(
                    Setting::restore(
                        SettingKey::new("tenantscoped").unwrap(),
                        SettingValue::new(serde_json::json!({"sensitive": true})).unwrap(),
                        tenant,
                        1,
                        now,
                    )
                    .unwrap(),
                );
            }
            // GET /settings/ search happy path: a tenant-scoped row the real
            // `list_settings` maps through `SettingResponse` (query=theme).
            "settings exist for the authenticated user's tenant" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({ "color": "dark" })).unwrap(),
                        tenant,
                        1,
                        now,
                    )
                    .unwrap(),
                );
            }
            // Two tenant-scoped rows (theme + lang) for the NDJSON 2-line
            // interaction — the real `list_settings` maps them in insertion
            // order, so line 1 = theme, line 2 = lang.
            "two settings exist for the authenticated user's tenant" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({ "color": "dark" })).unwrap(),
                        tenant.clone(),
                        1,
                        now,
                    )
                    .unwrap(),
                );
                self.settings.seed(
                    Setting::restore(
                        SettingKey::new("theme2").unwrap(),
                        SettingValue::new(serde_json::json!({ "theme": "dark" })).unwrap(),
                        tenant.clone(),
                        1,
                        now,
                    )
                    .unwrap(),
                );
            }
            // PUT on a duplicate (tenant,user,key) -> repository Conflict -> 409
            // settings:value_conflict.
            "a setting already exists for the authenticated user" => {
                self.settings.seed(
                    Setting::restore(
                        theme,
                        SettingValue::new(serde_json::json!({ "color": "dark" })).unwrap(),
                        tenant,
                        1,
                        now,
                    )
                    .unwrap(),
                );
                self.settings
                    .conflict_on_insert
                    .store(true, Ordering::SeqCst);
            }
            // No provider state (e.g. the 401 interaction). The invalid_key,
            // invalid_value and pagination:bad_page interactions fail before the
            // service port is reached, so they need no seeding.
            _ => {}
        }
    }
}

#[async_trait]
impl ProviderStateExecutor for ProviderStateSeeding {
    async fn call(
        self: Arc<Self>,
        _interaction_id: Option<String>,
        provider_state: &ProviderState,
        setup: bool,
        _client: Option<&pact_reqwest::Client>,
    ) -> anyhow::Result<HashMap<String, serde_json::Value>> {
        // Reset + seed on setup; reset on teardown (no state leaks either way).
        if setup {
            self.seed_for(&provider_state.name);
        } else {
            self.settings.reset();
        }
        Ok(HashMap::new())
    }

    fn teardown(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// The verification itself.
// ---------------------------------------------------------------------------

/// Verify the committed contract against the real REST adapter, in-process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pact_provider_verifies_the_rest_adapter_in_process() {
    let settings = StubSettingsService::new();

    let config = Config {
        database_url: "postgres://unused".to_string(),
        service_host: "127.0.0.1".to_string(),
        service_port: 0,
        allowed_origins: vec!["http://localhost:3000".to_string()],
        auth_issuer: "http://idp.example/realm".to_string(),
        auth_audience: "settings-service".to_string(),
        service_name: "settings-service".to_string(),
        otlp_endpoint: "http://localhost:4317".to_string(),
        otel_http_endpoint: "http://localhost:4318".to_string(),
        rate_limit_requests: 100_000,
        rate_limit_burst: 200_000,
    };

    // Real router with the settings primary-port stub + mock JWT validator +
    // in-memory revocation store. DB-less and IdP-less adapter-contract test.
    let auth = AuthService::new(PactJwtValidator::new(), InMemoryRevocationStore::new());
    let state = AppState::new(
        settings.clone(),
        auth,
        RateLimiter::new(
            config.rate_limit_requests,
            config.rate_limit_burst,
            Duration::from_secs(60),
        ),
        config.otel_http_endpoint.clone(),
        prometheus::Registry::new(),
        reqwest::Client::new(),
    );
    let router = build_router(&config, state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });

    #[allow(deprecated)]
    let provider_info = ProviderInfo {
        name: "settings-service".to_string(),
        protocol: "http".to_string(),
        host: "127.0.0.1".to_string(),
        port: Some(port),
        path: "/".to_string(),
        transports: vec![ProviderTransport {
            transport: "http".to_string(),
            port: Some(port),
            path: Some("/".to_string()),
            scheme: Some("http".to_string()),
        }],
    };

    let source = vec![PactSource::File("pact/settings-service.json".to_string())];
    let options = VerificationOptions::<NullRequestFilterExecutor>::default();
    let executor = Arc::new(ProviderStateSeeding { settings });

    let result = verify_provider_async(
        provider_info,
        source,
        FilterInfo::None,
        vec![],
        &options,
        None,
        &executor,
        None,
    )
    .await
    .expect("verification ran");

    println!("{}", result.output.join("\n"));

    let total = result.interaction_results.len();
    assert_eq!(
        total, 18,
        "expected all 18 pact interactions to be verified"
    );
    for r in &result.interaction_results {
        let status = if r.result.is_ok() { "OK" } else { "FAIL" };
        println!("[{status}] {}", r.interaction_description);
    }
    let failures: Vec<String> = result
        .interaction_results
        .iter()
        .filter(|r| r.result.is_err())
        .map(|r| r.interaction_description.clone())
        .collect();
    assert!(
        failures.is_empty(),
        "pact interactions failed: {failures:?}\n\n{}",
        result.output.join("\n")
    );
}
