//! Environment-driven configuration, validated eagerly and fail-fast.
//!
//! Every variable is read from `std::env` at startup in a single `from_env()`
//! that returns `Result<Self, ConfigError>`. There is no config file, no baked-in
//! secret, and no silent default for a security-critical value (`DATABASE_URL`,
//! `AUTH_ISSUER`, `AUTH_AUDIENCE`). CORS is an explicit allow-list (C3) — `*`
//! is never honored.

use std::env;
use std::fmt;
use std::net::SocketAddr;

/// Validated application configuration.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub service_host: String,
    pub service_port: u16,
    pub allowed_origins: Vec<String>,
    pub auth_issuer: String,
    pub auth_audience: String,
    pub service_name: String,
    pub otlp_endpoint: String,
    pub otel_http_endpoint: String,
    pub rate_limit_requests: u32,
    pub rate_limit_burst: u32,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
    /// Demo config-change interval in seconds; `0` disables the demo loop.
    pub demo_interval_secs: u64,
    /// Shared token that enables the demo REST trigger (`POST /demo/config/:key`).
    /// `None` disables the route entirely. Never a production auth mechanism.
    pub demo_trigger_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url =
            env::var("DATABASE_URL").map_err(ConfigError::missing("DATABASE_URL"))?;
        let auth_issuer = env::var("AUTH_ISSUER").map_err(ConfigError::missing("AUTH_ISSUER"))?;
        let auth_audience =
            env::var("AUTH_AUDIENCE").map_err(ConfigError::missing("AUTH_AUDIENCE"))?;

        if database_url.is_empty() || auth_issuer.is_empty() || auth_audience.is_empty() {
            return Err(ConfigError::new(
                "DATABASE_URL/AUTH_ISSUER/AUTH_AUDIENCE",
                "must not be empty",
            ));
        }

        let service_host = env::var("SERVICE_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let service_port = env::var("SERVICE_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);
        let allowed_origins = comma_list(
            &env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:3000".into()),
        );
        let service_name =
            env::var("SERVICE_NAME").unwrap_or_else(|_| "settings-service".to_string());
        let otlp_endpoint =
            env::var("OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".to_string());
        let otel_http_endpoint =
            env::var("OTEL_HTTP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string());
        let rate_limit_requests = env::var("RATE_LIMIT_REQUESTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let rate_limit_burst = env::var("RATE_LIMIT_BURST")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        let mqtt_host = env::var("MQTT_HOST").unwrap_or_else(|_| "mosquitto".to_string());
        let mqtt_port = env::var("MQTT_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1883);
        let mqtt_client_id = env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| service_name.clone());
        let demo_interval_secs = env::var("DEMO_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let demo_trigger_token = env::var("DEMO_TRIGGER_TOKEN").ok();

        Ok(Self {
            database_url,
            service_host,
            service_port,
            allowed_origins,
            auth_issuer,
            auth_audience,
            service_name,
            otlp_endpoint,
            otel_http_endpoint,
            rate_limit_requests,
            rate_limit_burst,
            mqtt_host,
            mqtt_port,
            mqtt_client_id,
            demo_interval_secs,
            demo_trigger_token,
        })
    }

    /// The socket address the HTTP server binds. Validated once here.
    pub fn bind_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.service_host, self.service_port)
            .parse()
            .map_err(|_| ConfigError::new("SERVICE_HOST:SERVICE_PORT", "invalid bind address"))
    }

    /// CORS allow-list. The wildcard `*` is never honored (C3).
    pub fn cors_allows(&self) -> &[String] {
        &self.allowed_origins
    }
}

fn comma_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "*")
        .map(str::to_string)
        .collect()
}

/// Fail-fast configuration error (shown at startup; never a response body).
#[derive(Debug)]
pub struct ConfigError {
    pub variable: String,
    pub reason: String,
}

impl ConfigError {
    fn missing(variable: &'static str) -> impl FnOnce(env::VarError) -> Self {
        move |_| Self {
            variable: variable.to_string(),
            reason: "required variable is not set".to_string(),
        }
    }

    fn new(variable: &str, reason: &str) -> Self {
        Self {
            variable: variable.to_string(),
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid config '{}': {}", self.variable, self.reason)
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_origin_never_honored() {
        assert!(comma_list("*").is_empty());
        assert_eq!(
            comma_list("http://a, *, http://b"),
            vec!["http://a".to_string(), "http://b".to_string()]
        );
    }
}
