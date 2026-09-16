//! Composition root — the only place concrete adapters are named.
//!
//! Assembles `Config` (fail-fast), telemetry, the sqlx pool, the settings and
//! auth services with their concrete adapters, the `AppState`, CORS, security
//! headers and observability layers, and serves. Migrations are NOT applied
//! here (D1/D11) — they run in a dedicated `just migrate` / CI stage.
//!
//! The router itself lives in the library (`settings_service::router::build_router`),
//! so the in-process adapter-contract contract tests verify the exact same HTTP
//! surface as production (no duplicate router).

use std::time::Duration;

use anyhow::Context;
use settings_service::auth::{AuthService, InMemoryRevocationStore, JwtAuthAdapter};
use settings_service::common::config::Config;
use settings_service::common::rate_limit::RateLimiter;
use settings_service::notify::{self, MqttNotifier, NotifyingSettingsService};
use settings_service::router::build_router;
use settings_service::settings::{PgSettingsRepository, SettingsService};
use settings_service::state::AppState;

// FROM scratch runtime (Option 2): mimalloc is a drop-in high-throughput
// allocator that beats musl's malloc under multi-threaded Tokio load. The
// crate handles the unsafe internals; declaring a global allocator is safe.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!("{e}"))?;

    let _telemetry =
        settings_service::common::telemetry::init(&config).map_err(anyhow::Error::msg)?;

    // OTLP span export + W3C trace-context propagation over MQTT 5 User Properties.
    notify::otel::init(&config.otel_http_endpoint, &config.service_name);

    // Persistence adapter — schema is already migrated (never at startup, D1).
    let pool = setup_database(&config.database_url).await?;
    let settings = SettingsService::new(PgSettingsRepository::new(pool));

    // Config-change notifications: one MQTT client + event loop per instance;
    // the loop also receives other instances' notifications (trace-continuing).
    let (mqtt, eventloop) = notify::mqtt_client(&config);
    tokio::spawn(notify::run_event_loop(
        eventloop,
        mqtt.clone(),
        config.service_name.clone(),
    ));
    let settings = NotifyingSettingsService::new(
        settings,
        MqttNotifier::new(mqtt),
        config.service_name.clone(),
    );

    // IdP: Keycloak locally / fallback; Entra ID transparent (D9). JWKS endpoint
    // is derived from the issuer — swapping IdPs needs no app code change.
    let jwks_url = format!(
        "{}/protocol/openid-connect/certs",
        config.auth_issuer.trim_end_matches('/')
    );
    let auth_service = AuthService::new(
        JwtAuthAdapter::new(
            config.auth_issuer.clone(),
            config.auth_audience.clone(),
            jwks_url,
        ),
        InMemoryRevocationStore::new(),
    );

    let reqwest = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .dns_resolver(settings_service::common::dns::reqwest_resolver())
        .build()?;
    let state = AppState::new(
        settings,
        auth_service,
        RateLimiter::new(
            config.rate_limit_requests,
            config.rate_limit_burst,
            Duration::from_secs(1),
        ),
        config.otel_http_endpoint.clone(),
        prometheus::Registry::new(),
        reqwest,
    );

    // Optional demo loop: each instance periodically changes its own config key,
    // which the decorator turns into an MQTT notification (with trace context).
    if config.demo_interval_secs > 0 {
        tokio::spawn(notify::demo_publisher(
            state.settings.clone(),
            Duration::from_secs(config.demo_interval_secs),
            "acme".to_string(),
            format!("demo/{}", config.service_name),
            config.service_name.clone(),
        ));
    }

    // Flush exported spans off the event-loop thread (a blocking export on the
    // loop starves the MQTT keepalive and the broker drops the session).
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _ = tokio::task::spawn_blocking(settings_service::notify::otel::flush).await;
        }
    });

    let app = build_router(&config, state);

    let addr = config.bind_addr()?;
    tracing::info!(%addr, service = %config.service_name, "serving");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Connect to Postgres with a bounded retry (C5), so the app tolerates an infra
/// dependency still waking up — but fails after a cap.
async fn setup_database(database_url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut last = None;
    for attempt in 0..10 {
        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                last = Some(e);
                tracing::warn!(attempt, "database connect failed, retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(anyhow::anyhow!(
        "database unavailable after retries: {last:?}"
    ))
    .with_context(|| "could not connect to Postgres")
}
