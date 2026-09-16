//! The `settings` slice's HTTP adapter: handlers, adapter-local request/response
//! types (no global `dto` crate), and `From<SettingsError> for ApiError`.
//!
//! The domain stays serde-free — this adapter maps the `SettingSnapshot` value
//! object into a `SettingResponse` transport type. Optimistic concurrency
//! (be-arch §10) is exposed as `ETag` on reads and `If-Match` on writes; a
//! stale precondition -> 409.

use std::time::Instant;

use axum::extract::{Extension, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::auth::{Authority, JwtValidator, RevocationStore};
use crate::common::error::ApiError;
use crate::common::pagination::Pagination;
use crate::settings::error::SettingsError;
use crate::settings::model::{Setting, SettingKey, SettingSnapshot, SettingValue};
use crate::settings::settings_port::SettingsServicePort;
use crate::state::AppState;

/// Body for `PUT` / `PATCH` — the config value (JSON object).
#[derive(Debug, serde::Deserialize)]
pub struct SettingUpsert {
    pub value: serde_json::Value,
}

/// Adapter-local read model (no `*Dto` suffix, no global dto crate).
#[derive(Debug, Serialize)]
pub struct SettingResponse {
    pub key: String,
    pub value: serde_json::Value,
    pub owner: OwnerResponse,
    pub version: u64,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct OwnerResponse {
    pub tenant: String,
    pub user: Option<String>,
    pub scope: String,
}

impl From<SettingSnapshot> for SettingResponse {
    fn from(s: SettingSnapshot) -> Self {
        let owner = OwnerResponse {
            tenant: s.ownership.tenant().as_str().to_string(),
            user: s.ownership.user().map(|u| u.as_str().to_string()),
            scope: if s.ownership.is_tenant_scoped() {
                "tenant"
            } else {
                "user"
            }
            .to_string(),
        };
        Self {
            key: s.key.as_str().to_string(),
            value: s.value.as_value().clone(),
            owner,
            version: s.version,
            updated_at: instant_updated_at(s.updated_at),
        }
    }
}

/// GET /settings?query=&page=&size= — fuzzy search, real pagination.
#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    pub query: Option<String>,
    pub page: Option<u32>,
    pub size: Option<u32>,
}

/// Build the `/settings` router for the generic state `AppState<J, Rv, S>`
/// (the settings **primary** port, not a repository).
pub fn settings_routes<J, Rv, S>() -> Router<AppState<J, Rv, S>>
where
    J: JwtValidator + Send + Sync + 'static,
    Rv: RevocationStore + Send + Sync + 'static,
    S: SettingsServicePort + Send + Sync + 'static,
{
    Router::<AppState<J, Rv, S>>::new()
        .route("/", get(list_settings))
        .route(
            "/:key",
            get(get_setting)
                .put(put_setting)
                .patch(patch_setting)
                .delete(delete_setting),
        )
}

pub async fn list_settings<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(auth): Extension<Authority>,
    Query(q): Query<ListQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let pagination =
        Pagination::new(q.page.unwrap_or(1), q.size.unwrap_or(20)).map_err(ApiError::from)?;
    let page = state
        .settings
        .search(q.query.as_deref().unwrap_or(""), &auth, &pagination)
        .await
        .map_err(ApiError::from)?;

    // The DTO mapping is shared by both the JSON-array and NDJSON encodings, so
    // the object shape is byte-identical in either format.
    let rows: Vec<SettingResponse> = page
        .rows
        .into_iter()
        .map(|s| s.snapshot())
        .map(SettingResponse::from)
        .collect();

    // Content negotiation: `Accept: application/x-ndjson` (or the synonyms
    // `application/jsonl` / `ndjson`) selects newline-delimited JSON; otherwise
    // (no Accept, `application/json`, `*/*`) the default JSON array is returned.
    let mut resp = if accepts_ndjson(&headers) {
        let body = rows
            .iter()
            .map(|r| serde_json::to_string(r).expect("SettingResponse serializes"))
            .collect::<Vec<_>>()
            .join("\n");
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/x-ndjson")
            .body(axum::body::Body::from(body))
            .unwrap()
    } else {
        Json(rows).into_response()
    };

    // RFC 8288 Web-Linking pagination headers on the collection response. The
    // URL references are relative (RFC 3986) against the `/settings` request
    // path; only `page` varies, the original `query`/`size` params survive.
    let link = build_link_header(
        pagination.page(),
        pagination.size(),
        page.total,
        q.query.as_deref(),
    );
    let h = resp.headers_mut();
    h.insert(
        header::LINK,
        HeaderValue::from_str(&link).expect("Link header value is a valid header"),
    );
    h.insert(
        "x-total-count",
        HeaderValue::from_str(&page.total.to_string()).expect("total is a valid header value"),
    );
    Ok(resp)
}

/// Does the request's `Accept` header request newline-delimited JSON?
/// Accepts `application/x-ndjson` plus the synonyms `application/jsonl` and
/// the bare `ndjson`. Media ranges are comma-separated and may carry params
/// (e.g. `application/x-ndjson; charset=utf-8`), which we strip.
fn accepts_ndjson(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|accept| {
            accept.split(',').any(|media_range| {
                let media_type = media_range.split(';').next().unwrap_or("").trim();
                matches!(
                    media_type.to_ascii_lowercase().as_str(),
                    "application/x-ndjson" | "application/jsonl" | "ndjson"
                )
            })
        })
        .unwrap_or(false)
}

/// Build an RFC 8288 `Link` header with relative URI references (RFC 3986),
/// composed from the ACTUAL validated `page`/`size` and the `total` of the same
/// filtered set. `query` (the fuzzy filter) and `size` are preserved verbatim;
/// only `page` changes between links. `first` is always present; `prev` only
/// when `page > 1`; `next` only when a genuine next page exists
/// (`page*size < total`); `last` (ceil(total/size), min 1) only when total > 0.
fn build_link_header(page: u32, size: u32, total: u64, query: Option<&str>) -> String {
    let query_param = query.map(|q| format!("&query={q}")).unwrap_or_default();
    let mut links = Vec::with_capacity(4);

    links.push(format!(
        "<settings?page=1&size={size}{query_param}>; rel=\"first\""
    ));
    if page > 1 {
        links.push(format!(
            "<settings?page={}&size={size}{query_param}>; rel=\"prev\"",
            page - 1
        ));
    }
    if (page as u64) * (size as u64) < total {
        links.push(format!(
            "<settings?page={}&size={size}{query_param}>; rel=\"next\"",
            page + 1
        ));
    }
    if total > 0 {
        let last = total.div_ceil(size as u64).max(1) as u32;
        links.push(format!(
            "<settings?page={last}&size={size}{query_param}>; rel=\"last\""
        ));
    }
    links.join(", ")
}

pub async fn get_setting<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(auth): Extension<Authority>,
    Path(key): Path<String>,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let key = SettingKey::new(&key).map_err(ApiError::from)?;
    let setting = state
        .settings
        .get(&key, &auth)
        .await
        .map_err(ApiError::from)?;
    Ok(etag_response(setting))
}

pub async fn put_setting<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(auth): Extension<Authority>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<SettingUpsert>,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let key = SettingKey::new(&key).map_err(ApiError::from)?;
    let value = SettingValue::new(body.value).map_err(ApiError::from)?;
    let expected = if_match(&headers);
    let setting = state
        .settings
        .put(key, value, &auth, expected, Instant::now())
        .await
        .map_err(ApiError::from)?;
    Ok(etag_response(setting))
}

pub async fn patch_setting<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(auth): Extension<Authority>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<SettingUpsert>,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let key = SettingKey::new(&key).map_err(ApiError::from)?;
    let value = SettingValue::new(body.value).map_err(ApiError::from)?;
    let expected = if_match(&headers).ok_or_else(|| {
        ApiError::act(
            "settings:version_conflict",
            StatusCode::CONFLICT,
            "If-Match required",
        )
    })?;
    let setting = state
        .settings
        .patch(key, value, &auth, expected, Instant::now())
        .await
        .map_err(ApiError::from)?;
    Ok(etag_response(setting))
}

pub async fn delete_setting<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    Extension(auth): Extension<Authority>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let key = SettingKey::new(&key).map_err(ApiError::from)?;
    let expected = if_match(&headers).ok_or_else(|| {
        ApiError::act(
            "settings:version_conflict",
            StatusCode::CONFLICT,
            "If-Match required",
        )
    })?;
    state
        .settings
        .delete(key, &auth, expected)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `From<SettingsError> for ApiError` — the only place a settings error is an
/// HTTP status. Stable codes are the contract; retry bucket adds `Retry-After`.
impl From<SettingsError> for ApiError {
    fn from(e: SettingsError) -> Self {
        match e {
            SettingsError::NotFound => Self::act(
                "settings:not_found",
                StatusCode::NOT_FOUND,
                "Setting not found",
            ),
            SettingsError::InvalidKey => Self::act(
                "settings:invalid_key",
                StatusCode::BAD_REQUEST,
                "Invalid key",
            ),
            SettingsError::InvalidValue => Self::act(
                "settings:invalid_value",
                StatusCode::UNPROCESSABLE_ENTITY,
                "value must be a JSON object under 64 KB",
            ),
            SettingsError::Conflict => Self::act(
                "settings:value_conflict",
                StatusCode::CONFLICT,
                "Setting already exists",
            ),
            SettingsError::VersionConflict => Self::act(
                "settings:version_conflict",
                StatusCode::CONFLICT,
                "Precondition failed — setting changed",
            ),
            SettingsError::Forbidden => Self::act(
                "settings:forbidden",
                StatusCode::FORBIDDEN,
                "Not authorized for this setting",
            ),
            SettingsError::RateLimited => {
                Self::retry("settings:rate_limited", StatusCode::TOO_MANY_REQUESTS, 1)
            }
            SettingsError::Unavailable => {
                Self::retry("settings:unavailable", StatusCode::SERVICE_UNAVAILABLE, 5)
            }
            SettingsError::Internal => Self::bug("settings:internal", "settings internal error"),
        }
    }
}

fn etag_response(setting: Setting) -> Response {
    let snapshot = setting.snapshot();
    let etag = snapshot.version.to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, format!("\"{etag}\""))
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&SettingResponse::from(snapshot)).unwrap(),
        ))
        .unwrap()
}

fn if_match(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().trim_matches('"').parse().ok())
}

/// `Instant` -> RFC-3339 UTC string with **sub-seconds**, e.g.
/// `"2023-11-14T22:13:20.123Z"`. The epoch-millis is derived from the single
/// anchored `pg_settings::instant_to_millis` base (captured once via `OnceLock`,
/// no per-call wall-clock drift) and formatted with sub-second precision.
fn instant_updated_at(i: Instant) -> String {
    let millis = super::pg_settings::instant_to_millis(i);
    DateTime::<Utc>::from_timestamp_millis(millis)
        .expect("epoch-millis always in chrono range")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}
