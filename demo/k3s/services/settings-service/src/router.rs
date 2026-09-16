//! The shared router assembly — the single place the HTTP surface is built.
//!
//! This lives in the **library** (not the binary) so the composition root
//! (`main.rs`) and the adapter-contract tests (`tests/pact_contract.rs`) build
//! the *same* router: identical routes, CORS allow-list, `require_auth`
//! middleware, security headers, trace layer and the same-origin telemetry
//! proxy. No unit-test router duplication (the pact harness verifies this exact
//! router). Generic over the ports, so the concrete adapters are injected by
//! the caller via `AppState`.

use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::{self, JwtValidator, RevocationStore};
use crate::common::config::Config;
use crate::common::error::ApiError;
use crate::settings::{settings_routes, SettingsServicePort};
use crate::state::AppState;

/// Assemble the full router: settings slice, auth logout, health, metrics and
/// the same-origin `/telemetry` proxy, with CORS allow-list + security headers.
pub fn build_router<J, Rv, S>(config: &Config, state: AppState<J, Rv, S>) -> Router
where
    J: JwtValidator + Send + Sync + 'static,
    Rv: RevocationStore + Send + Sync + 'static,
    S: SettingsServicePort + Send + Sync + 'static,
{
    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .map(|o| {
            o.parse::<HeaderValue>()
                .expect("configured allowed origin parses")
        })
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::POST,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::IF_MATCH,
        ])
        .allow_credentials(false);

    let mut router = Router::new()
        .nest(
            "/settings",
            settings_routes().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth::<J, Rv, S>,
            )),
        )
        .route("/auth/logout", post(auth::logout))
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/readyz", get(readyz::<J, Rv, S>))
        .route("/metrics", get(metrics_handler))
        .route("/telemetry", post(telemetry_proxy));

    // Demo-only REST trigger: curl -> config change -> MQTT notification. Absent
    // unless DEMO_TRIGGER_TOKEN is configured (see notify::demo_rest).
    if let Some(token) = &config.demo_trigger_token {
        router = router.merge(crate::notify::demo_routes::<J, Rv, S>(token.clone()));
    }

    router
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(state)
}

/// Set the security headers (C2) on every response — HSTS, nosniff, frame
/// denial, referrer policy, CSP, permissions policy.
async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    let entries: [(&str, &str); 6] = [
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'",
        ),
        (
            "permissions-policy",
            "geolocation=(), camera=(), microphone=()",
        ),
    ];
    for (name, value) in entries {
        let name = axum::http::HeaderName::from_static(name);
        let value = axum::http::HeaderValue::from_static(value);
        headers.insert(name, value);
    }
    resp
}

/// `GET /readyz` — readiness probe: 200 once the backing store (Postgres) is
/// reachable, 503 otherwise. K8s uses this to gate traffic; the Dockerfile has
/// no `curl`/HEALTHCHECK because on `FROM scratch` probes go via `httpGet`, not
/// an exec in-container.
async fn readyz<J, Rv, S>(State(state): State<AppState<J, Rv, S>>) -> Response
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    match state.settings.ready().await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "readiness probe failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// `GET /metrics` — Prometheus text exposition from the app's registry.
async fn metrics_handler<J, Rv, S>(State(state): State<AppState<J, Rv, S>>) -> Response
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let families = state.metrics.gather();
    let mut buf = Vec::new();
    if encoder.encode(&families, &mut buf).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
        .body(Body::from(buf))
        .unwrap()
}

/// Same-origin OTLP proxy (deployment §OTel): a browser/native client POSTs
/// OTLP/HTTP here with `Authorization: Bearer`, and we forward it to the
/// Collector's OTLP HTTP receiver.
async fn telemetry_proxy<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    // Require a bearer token (A11 default-deny) so traces carry the identity.
    let _ = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(ApiError::unauthorized)?;
    let url = format!(
        "{}/v1/traces",
        state.otel_http_endpoint.trim_end_matches('/')
    );
    let forwarded = state
        .reqwest
        .post(&url)
        .header(header::CONTENT_TYPE, "application/x-protobuf")
        .body(body.to_vec())
        .send()
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .unwrap_or(StatusCode::BAD_GATEWAY);
    Ok(forwarded.into_response())
}
