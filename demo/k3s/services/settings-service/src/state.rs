//! The shared router `State` — the wiring shape used by every handler.
//!
//! Composition root (`main.rs`) constructs this once with concrete adapters;
//! the state itself is generic over the ports, so static dispatch (generics)
//! propagates cleanly into handlers (ADR-003).

use std::sync::Arc;

use crate::auth::{AuthService, JwtValidator, RevocationStore};
use crate::common::rate_limit::RateLimiter;
use crate::settings::SettingsServicePort;

/// The ports the router depends on: the settings **primary** port (`S`, mocked
/// at this boundary in adapter tests) and the two auth ports (`JwtValidator` +
/// `RevocationStore`). The repository port (`R`) is *not* part of the router
/// state — it lives only inside the concrete `SettingsService<R>` that
/// implements `SettingsServicePort`, so the REST adapter is insulated from
/// persistence. Not a bundle trait.
///
/// `Clone` is implemented manually (fields are `Arc`/`String`/`Registry`), so
/// axum's `State` extraction does not require `J`/`Rv`/`S` to be `Clone`.
pub struct AppState<J: JwtValidator, Rv: RevocationStore, S: SettingsServicePort> {
    pub settings: Arc<S>,
    pub auth: Arc<AuthService<J, Rv>>,
    pub rate: Arc<RateLimiter>,
    pub otel_http_endpoint: String,
    pub metrics: Arc<prometheus::Registry>,
    pub reqwest: reqwest::Client,
}

impl<J: JwtValidator, Rv: RevocationStore, S: SettingsServicePort> Clone for AppState<J, Rv, S> {
    fn clone(&self) -> Self {
        Self {
            settings: Arc::clone(&self.settings),
            auth: Arc::clone(&self.auth),
            rate: Arc::clone(&self.rate),
            otel_http_endpoint: self.otel_http_endpoint.clone(),
            metrics: Arc::clone(&self.metrics),
            reqwest: self.reqwest.clone(),
        }
    }
}

impl<J: JwtValidator, Rv: RevocationStore, S: SettingsServicePort> AppState<J, Rv, S> {
    pub fn new(
        settings: S,
        auth: AuthService<J, Rv>,
        rate: RateLimiter,
        otel_http_endpoint: String,
        metrics: prometheus::Registry,
        reqwest: reqwest::Client,
    ) -> Self {
        Self {
            settings: Arc::new(settings),
            auth: Arc::new(auth),
            rate: Arc::new(rate),
            otel_http_endpoint,
            metrics: Arc::new(metrics),
            reqwest,
        }
    }
}
