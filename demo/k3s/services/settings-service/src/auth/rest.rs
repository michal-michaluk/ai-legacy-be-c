//! The `auth` slice's HTTP adapter: the default-deny `require_auth` middleware
//! (A11), the `/auth/logout` handler (A14), and `From<AuthError> for ApiError`.
//!
//! The middleware validates the bearer token, inserts the domain `Authority`
//! into request extensions (a request-scoped value downstream handlers read as
//! `Extension<Authority>`), and rate-limits (A9). Default-deny: an unauthenticated
//! request is rejected before any handler runs.

use axum::extract::{Request, State};
use axum::http::header;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::error::AuthError;
use crate::auth::{JwtValidator, RevocationStore};
use crate::common::error::ApiError;
use crate::settings::SettingsServicePort;
use crate::state::AppState;

/// Default-deny middleware: require a valid bearer token, resolve it into an
/// `Authority`, rate-limit, and insert it into the request extensions.
pub async fn require_auth<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let token = bearer_token(req.headers()).ok_or_else(ApiError::unauthorized)?;
    let authority = state
        .auth
        .validate_token(token)
        .await
        .map_err(ApiError::from)?;
    if !state
        .rate
        .allow(&format!("tenant:{}", authority.tenant().as_str()))
    {
        return Err(ApiError::retry(
            "settings:rate_limited",
            StatusCode::TOO_MANY_REQUESTS,
            1,
        ));
    }
    req.extensions_mut().insert(authority);
    Ok(next.run(req).await)
}

/// POST /auth/logout — revoke the token's `jti` for the remaining TTL (A14).
pub async fn logout<J, Rv, S>(
    State(state): State<AppState<J, Rv, S>>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError>
where
    J: JwtValidator,
    Rv: RevocationStore,
    S: SettingsServicePort,
{
    let token = bearer_token(&headers).ok_or_else(ApiError::unauthorized)?;
    let authority = state
        .auth
        .validate_token(token)
        .await
        .map_err(ApiError::from)?;
    // Rate-limit the auth endpoint (A9) once the caller is identified.
    if !state
        .rate
        .allow(&format!("auth:{}", authority.tenant().as_str()))
    {
        return Err(ApiError::retry(
            "auth:rate_limited",
            StatusCode::TOO_MANY_REQUESTS,
            1,
        ));
    }
    state.auth.logout(token).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `From<AuthError> for ApiError` — the only place an auth error is an HTTP
/// status. JWT failures are the act bucket (401); revoked is 401; rate-limit is
/// the retry bucket. Internal is opaque (A10).
impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::MissingCredentials => Self::act(
                "auth:missing_credentials",
                StatusCode::UNAUTHORIZED,
                "Authentication required",
            ),
            AuthError::InvalidToken => Self::act(
                "auth:invalid_token",
                StatusCode::UNAUTHORIZED,
                "Invalid token",
            ),
            AuthError::Expired => {
                Self::act("auth:expired", StatusCode::UNAUTHORIZED, "Token expired")
            }
            AuthError::InvalidIssuer => Self::act(
                "auth:invalid_issuer",
                StatusCode::UNAUTHORIZED,
                "Invalid issuer",
            ),
            AuthError::InvalidAudience => Self::act(
                "auth:invalid_audience",
                StatusCode::UNAUTHORIZED,
                "Invalid audience",
            ),
            AuthError::Revoked => {
                Self::act("auth:revoked", StatusCode::UNAUTHORIZED, "Token revoked")
            }
            AuthError::RateLimited => {
                Self::retry("auth:rate_limited", StatusCode::TOO_MANY_REQUESTS, 1)
            }
            AuthError::Unavailable => {
                Self::retry("auth:unavailable", StatusCode::SERVICE_UNAVAILABLE, 5)
            }
            AuthError::Internal => {
                tracing::error!(error = %e, "auth internal error");
                Self::bug("auth:internal", "auth internal error")
            }
        }
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token)
    } else {
        None
    }
}

// Keep `IntoResponse` referenced so the rejection type is `ApiError -> Response`.
#[allow(dead_code)]
fn _rejection_is_response(e: ApiError) -> Response {
    e.into_response()
}
