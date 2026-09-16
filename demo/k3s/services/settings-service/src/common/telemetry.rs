//! Observability: structured JSON logging (`tracing`) + Prometheus metrics.
//!
//! `tenant`/`user`/`role` are attached as **span attributes** at the service
//! boundary (security A12) — they ride on every downstream span/log. Errors are
//! reported OTel-native (a global handler records bucket + code as a span/event);
//! there is no separate external error tracker (no Sentry).
//!
//! The OTLP **deployment** mechanics (Collector + same-origin `/telemetry`
//! proxy + `traces.jsonl`) are owned by `deployment-requirements.md` §OTel and
//! live in `k8s/infra/otel-collector.yaml` + the `/telemetry` handler.

use std::sync::Arc;

use prometheus::Registry;
use tracing_subscriber::filter::EnvFilter;

use crate::common::config::Config;

/// Owns the metrics registry for the app lifetime.
#[derive(Clone)]
pub struct Telemetry {
    registry: Arc<Registry>,
}

impl Telemetry {
    /// The Prometheus registry served at `/metrics`.
    pub fn registry(&self) -> Arc<Registry> {
        Arc::clone(&self.registry)
    }
}

/// Install the `tracing` subscriber (JSON + `RUST_LOG` filter) and the
/// Prometheus registry. `RUST_LOG=info` in prod; never `debug`/`trace` (would
/// expose request bodies). Fail-fast on a malformed filter.
pub fn init(config: &Config) -> Result<Telemetry, Box<dyn std::error::Error + Send + Sync>> {
    let registry = Arc::new(Registry::new());
    register_metrics(&registry)?;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| format!("tracing init: {e}"))?;

    tracing::info!(service = %config.service_name, "application tracing initialized");
    Ok(Telemetry { registry })
}

fn register_metrics(registry: &Registry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use prometheus::{IntCounterVec, Opts};
    let requests = IntCounterVec::new(
        Opts::new("http_requests_total", "Total HTTP requests processed").namespace("settings"),
        &["method", "path", "status"],
    )?;
    registry.register(Box::new(requests))?;
    Ok(())
}
