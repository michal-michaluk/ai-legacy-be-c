//! JWKS resource-server adapter (`JwtAuthAdapter`) — the **only** place raw
//! JWT / claims are parsed (A1). Validates signature + issuer + audience
//! against the IdP's JWKS (A15), caches keys with a TTL and keeps the previous
//! set as a fallback on rotation (A13), and maps the token to a domain
//! `Authority` / `JwtPrincipal`.
//!
//! Keycloak is the IdP locally / fallback; swapping to Entra ID is transparent —
//! this adapter depends only on the token + JWKS, never on the IdP product (D9).

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::Deserialize;
use tracing::instrument;

use crate::auth::auth_port::{JwtPrincipal, JwtValidator};
use crate::auth::error::AuthError;
use crate::auth::model::{Authority, Role, Tenant, UserId};

/// How long a fetched JWKS set is trusted before it may be refreshed.
const JWKS_TTL: Duration = Duration::from_secs(300);

/// A cache of JWK sets keyed by `kid`, with a previous (rotated) set kept as a
/// fallback so a key rotation causes no downtime (A13).
struct JwksCache {
    url: String,
    client: reqwest::Client,
    keys: std::sync::RwLock<HashMap<String, Jwk>>,
    previous: std::sync::RwLock<HashMap<String, Jwk>>,
    fetched_at: std::sync::RwLock<Instant>,
}

impl JwksCache {
    fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client"),
            keys: std::sync::RwLock::new(HashMap::new()),
            previous: std::sync::RwLock::new(HashMap::new()),
            fetched_at: std::sync::RwLock::new(Instant::now() - JWKS_TTL),
        }
    }

    /// Return the decoding key for `kid`, refreshing from the IdP when stale or
    /// unknown — keeping the previous set as a fallback (A13).
    async fn key_for(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        if let Some(k) = key_from_map(&self.keys, kid) {
            return Ok(k);
        }
        if let Some(k) = key_from_map(&self.previous, kid) {
            return Ok(k);
        }
        self.refresh().await?;
        if let Some(k) = key_from_map(&self.keys, kid) {
            return Ok(k);
        }
        if let Some(k) = key_from_map(&self.previous, kid) {
            return Ok(k);
        }
        Err(AuthError::InvalidToken)
    }

    async fn refresh(&self) -> Result<(), AuthError> {
        let set: JwkSet = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|_| AuthError::Unavailable)?
            .error_for_status()
            .map_err(|_| AuthError::Unavailable)?
            .json()
            .await
            .map_err(|_| AuthError::Unavailable)?;

        let mut next = HashMap::new();
        for jwk in set.keys {
            if let Some(kid) = &jwk.common.key_id {
                next.insert(kid.clone(), jwk);
            }
        }
        if next.is_empty() {
            return Err(AuthError::Unavailable);
        }
        if let Ok(cur) = self.keys.read() {
            if let Ok(mut prev) = self.previous.write() {
                *prev = cur.clone();
            }
        }
        *self.keys.write().map_err(|_| AuthError::Internal)? = next;
        *self.fetched_at.write().map_err(|_| AuthError::Internal)? = Instant::now();
        Ok(())
    }
}

fn key_from_map(map: &std::sync::RwLock<HashMap<String, Jwk>>, kid: &str) -> Option<DecodingKey> {
    let guard = map.read().ok()?;
    let jwk = guard.get(kid)?;
    DecodingKey::from_jwk(jwk).ok()
}

/// Resource-server adapter. Holds only the JWKS endpoint + issuer + audience —
/// never self-signs or stores credentials (A17).
pub struct JwtAuthAdapter {
    issuer: String,
    audience: String,
    jwks: JwksCache,
}

impl JwtAuthAdapter {
    /// `jwks_url` = `{issuer}/protocol/openid-connect/certs` for Keycloak.
    pub fn new(issuer: String, audience: String, jwks_url: String) -> Self {
        Self {
            issuer,
            audience,
            jwks: JwksCache::new(jwks_url),
        }
    }
}

/// The subset of claims the service reads. The rest is ignored.
#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    exp: Option<u64>,
    #[serde(default)]
    tenant: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

impl JwtValidator for JwtAuthAdapter {
    #[instrument(skip(self, token))]
    async fn validate(&self, token: &str) -> Result<JwtPrincipal, AuthError> {
        let header = decode_header(token).map_err(|_| AuthError::InvalidToken)?;
        let kid = header.kid.as_deref().ok_or(AuthError::InvalidToken)?;
        let key = self.jwks.key_for(kid).await?;

        let mut validation = Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss"]);

        let data = decode::<Claims>(token, &key, &validation).map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
            _ => AuthError::InvalidToken,
        })?;
        let claims = data.claims;

        let user = claims
            .sub
            .or_else(|| claims.azp.clone())
            .ok_or(AuthError::InvalidToken)?;
        let tenant = claims.tenant.ok_or(AuthError::InvalidToken)?;
        let role = parse_role(claims.role.as_deref());

        let remaining_ttl = claims
            .exp
            .map(|exp| {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                Duration::from_secs(exp.saturating_sub(now))
            })
            .unwrap_or_default();

        let authority = Authority::new(Tenant::new(&tenant), UserId::new(&user), role);
        Ok(JwtPrincipal::new(
            authority,
            claims.jti.unwrap_or_default(),
            remaining_ttl,
        ))
    }
}

/// Role mapping. Unknown / `admin` -> Admin; otherwise User.
fn parse_role(s: Option<&str>) -> Role {
    match s.unwrap_or("User").to_ascii_lowercase().as_str() {
        "admin" | "administrator" => Role::Admin,
        _ => Role::User,
    }
}
