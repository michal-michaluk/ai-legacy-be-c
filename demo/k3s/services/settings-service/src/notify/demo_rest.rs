//! Demo-only REST trigger: a single authenticated-by-shared-token call that
//! changes a config key, which the notifying decorator then publishes over MQTT
//! to the other service instance.
//!
//! Enabled **only** when `DEMO_TRIGGER_TOKEN` is set; it is a demo harness, not
//! a production auth path (the real `/settings` API uses JWT/JWKS). The handler
//! starts an OTel `Server` span from the incoming W3C `traceparent` (or a new
//! root), so the whole chain REST -> MQTT -> other service is one trace.

use std::collections::HashMap;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};

use crate::auth::{Authority, JwtValidator, RevocationStore, Role, Tenant, UserId};
use crate::common::error::ApiError;
use crate::notify::otel;
use crate::settings::{SettingKey, SettingResponse, SettingValue, SettingsServicePort};
use crate::state::AppState;

/// Shared token injected as an `Extension` layer on the demo route.
#[derive(Clone)]
pub struct DemoToken(pub String);

#[derive(Debug, serde::Deserialize)]
pub struct DemoUpsert {
    value: serde_json::Value,
}

/// `POST /demo/config/:key` with header `X-Demo-Token: <DEMO_TRIGGER_TOKEN>`.
pub fn demo_routes<J, Rv, S>(token: String) -> Router<AppState<J, Rv, S>>
where
    J: JwtValidator + Send + Sync + 'static,
    Rv: RevocationStore + Send + Sync + 'static,
    S: SettingsServicePort + Send + Sync + 'static,
{
    Router::new()
        .route("/demo/config/:key", post(demo_config::<J, Rv, S>))
        .layer(Extension(DemoToken(token)))
}

async fn demo_config<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(DemoToken(token)): Extension<DemoToken>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DemoUpsert>,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let provided = headers
        .get("x-demo-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != token {
        return Err(ApiError::unauthorized());
    }

    let key = SettingKey::new(&key).map_err(ApiError::from)?;
    let value = SettingValue::new(body.value).map_err(ApiError::from)?;
    let authority = Authority::new(Tenant::new("acme"), UserId::new("demo-curl"), Role::Admin);

    let mut carrier = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            carrier.insert(name.as_str().to_lowercase(), v.to_string());
        }
    }
    let cx = otel::server_context("POST /demo/config", &carrier);
    let expected = state
        .settings
        .get(&key, &authority)
        .await
        .ok()
        .map(|s| s.version());
    let setting = otel::with_parent(
        cx.clone(),
        state
            .settings
            .put(key, value, &authority, expected, Instant::now()),
    )
    .await
    .map_err(ApiError::from)?;
    otel::end(&cx);

    Ok(Json(SettingResponse::from(setting.snapshot())).into_response())
}
