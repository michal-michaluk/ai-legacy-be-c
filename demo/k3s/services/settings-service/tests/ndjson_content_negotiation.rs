//! DB-less test of the `/settings` search endpoint's content negotiation and
//! pagination headers, via `tower::ServiceExt::oneshot` against the real
//! `settings_routes` built with a `SettingsServicePort` stub (no Postgres).
//!
//! This complements the Pact contract, which cannot reliably match a multi-line
//! NDJSON body — here we assert the wire format at the byte level: one
//! `SettingResponse` JSON object per line, and the JSON-array default is
//! unchanged.

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::Router;
use settings_service::auth::{
    Authority, InMemoryRevocationStore, JwtAuthAdapter, Ownership, Role, Tenant, UserId,
};
use settings_service::common::pagination::Pagination;
use settings_service::common::rate_limit::RateLimiter;
use settings_service::settings::{
    settings_routes, SearchPage, Setting, SettingKey, SettingValue, SettingsError,
    SettingsServicePort,
};
use settings_service::state::AppState;
use tower::ServiceExt;

/// Fixed `Instant` -> deterministic RFC-3339 `updated_at` (a sub-second,
/// past-anchored millis that round-trips exactly through the anchored base).
fn fixed_now() -> Instant {
    // 2100-01-01T00:00:00.123Z — safely in the future of the process's anchored
    // base, so the round-trip yields exactly these millis.
    settings_service::settings::millis_to_instant(4_102_444_800_123)
}

fn tenant_row(key: &str, value: serde_json::Value) -> Setting {
    Setting::restore(
        SettingKey::new(key).unwrap(),
        SettingValue::new(value).unwrap(),
        Ownership::for_tenant(Tenant::new("acme")),
        1,
        fixed_now(),
    )
    .unwrap()
}

/// A minimal `SettingsServicePort` double returning a fixed set of rows (with a
/// deterministic total derived from the SAME rows as the page).
struct StubSearch {
    rows: Vec<Setting>,
}

impl StubSearch {
    fn new(rows: Vec<Setting>) -> Self {
        Self { rows }
    }
}

impl SettingsServicePort for StubSearch {
    async fn get(
        &self,
        _key: &SettingKey,
        _authority: &Authority,
    ) -> Result<Setting, SettingsError> {
        Err(SettingsError::NotFound)
    }

    async fn search(
        &self,
        _query: &str,
        _authority: &Authority,
        pagination: &Pagination,
    ) -> Result<SearchPage, SettingsError> {
        let total = self.rows.len() as u64;
        let page_rows = self
            .rows
            .clone()
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
        _key: SettingKey,
        _value: SettingValue,
        _authority: &Authority,
        _expected: Option<u64>,
        _now: Instant,
    ) -> Result<Setting, SettingsError> {
        Err(SettingsError::Internal)
    }

    async fn patch(
        &self,
        _key: SettingKey,
        _patch: SettingValue,
        _authority: &Authority,
        _expected: u64,
        _now: Instant,
    ) -> Result<Setting, SettingsError> {
        Err(SettingsError::Internal)
    }

    async fn delete(
        &self,
        _key: SettingKey,
        _authority: &Authority,
        _expected: u64,
    ) -> Result<(), SettingsError> {
        Err(SettingsError::Internal)
    }

    async fn ready(&self) -> Result<(), SettingsError> {
        Ok(())
    }
}

fn build_app(rows: Vec<Setting>) -> Router {
    let settings = StubSearch::new(rows);
    let auth = settings_service::auth::AuthService::new(
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
        RateLimiter::new(100_000, 200_000, Duration::from_secs(60)),
        "http://localhost:4317".into(),
        prometheus::Registry::new(),
        reqwest::Client::new(),
    );
    settings_routes::<JwtAuthAdapter, InMemoryRevocationStore, StubSearch>().with_state(state)
}

async fn request(app: &Router, accept: Option<&str>) -> axum::http::Response<Body> {
    let mut builder = axum::http::Request::builder().method("GET").uri("/");
    if let Some(a) = accept {
        builder = builder.header(header::ACCEPT, a);
    }
    let mut req = builder.body(Body::empty()).unwrap();
    req.extensions_mut().insert(Authority::new(
        Tenant::new("acme"),
        UserId::new("alice"),
        Role::User,
    ));
    app.clone().oneshot(req).await.unwrap()
}

async fn body_string(resp: axum::http::Response<Body>) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn ndjson_accept_returns_one_setting_per_line() {
    let app = build_app(vec![
        tenant_row("theme", serde_json::json!({ "color": "dark" })),
        tenant_row("lang", serde_json::json!({ "locale": "pl-PL" })),
    ]);
    let resp = request(&app, Some("application/x-ndjson")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
    assert!(
        ct.as_bytes() == b"application/x-ndjson",
        "content-type = {ct:?}"
    );

    let body = body_string(resp).await;
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 2, "one SettingResponse object per line");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("line {line:?} is not valid JSON: {e}"));
        assert!(v.get("key").is_some(), "line is a SettingResponse: {v}");
        assert!(v.get("owner").is_some());
    }
}

#[tokio::test]
async fn ndjson_synonyms_are_accepted() {
    // `application/jsonl` and bare `ndjson` are treated as synonyms.
    let app = build_app(vec![tenant_row(
        "theme",
        serde_json::json!({ "color": "dark" }),
    )]);
    for accept in ["application/jsonl", "ndjson"] {
        let resp = request(&app, Some(accept)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            &HeaderValue::from_static("application/x-ndjson"),
            "accept = {accept}"
        );
        let body = body_string(resp).await;
        assert_eq!(body.lines().count(), 1);
    }
}

#[tokio::test]
async fn ndjson_empty_result_has_empty_body() {
    let app = build_app(Vec::new());
    let resp = request(&app, Some("application/x-ndjson")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        &HeaderValue::from_static("application/x-ndjson")
    );
    let body = body_string(resp).await;
    assert!(body.is_empty(), "empty result prefers an empty body");
}

#[tokio::test]
async fn default_accept_still_returns_a_json_array() {
    let app = build_app(vec![
        tenant_row("theme", serde_json::json!({ "color": "dark" })),
        tenant_row("lang", serde_json::json!({ "locale": "pl-PL" })),
    ]);
    // No Accept header -> default JSON array.
    let resp = request(&app, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        &HeaderValue::from_static("application/json")
    );
    let body = body_string(resp).await;
    let arr: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(arr.is_array(), "default body is a JSON array");
    assert_eq!(arr.as_array().unwrap().len(), 2);

    // Explicit application/json -> same array.
    let resp = request(&app, Some("application/json")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    let arr: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(arr.is_array());
    assert_eq!(arr.as_array().unwrap().len(), 2);
}
