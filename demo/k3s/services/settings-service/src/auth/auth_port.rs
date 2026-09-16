//! The `auth` slice's secondary ports — generic, native `async fn`, no
//! `#[async_trait]`, no trait objects, no bundle trait.
//!
//! `JwtValidator` turns a raw token into a validated principal; `RevocationStore`
//! implements server-side logout (A14). Concrete adapters (`jwt.rs`,
//! `revocation.rs`) are named only at the composition root.

use std::future::Future;
use std::time::Duration;

use crate::auth::error::AuthError;
use crate::auth::model::Authority;

/// A token that was validated (signature + iss + aud) and is safe to map to a
/// domain `Authority`. Carries the log-out metadata (A14): the session id `jti`
/// and the token's remaining TTL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JwtPrincipal {
    authority: Authority,
    jti: String,
    remaining_ttl: Duration,
}

impl JwtPrincipal {
    pub fn new(authority: Authority, jti: String, remaining_ttl: Duration) -> Self {
        Self {
            authority,
            jti,
            remaining_ttl,
        }
    }

    pub fn authority(&self) -> &Authority {
        &self.authority
    }

    pub fn jti(&self) -> &str {
        &self.jti
    }

    pub fn remaining_ttl(&self) -> Duration {
        self.remaining_ttl
    }
}

/// Validates a bearer token against the IdP's JWKS and maps it to a principal.
/// This is the **only** place raw JWT / claims are touched.
pub trait JwtValidator: Send + Sync {
    fn validate(&self, token: &str)
        -> impl Future<Output = Result<JwtPrincipal, AuthError>> + Send;
}

/// Server-side token blacklist (logout). A revoked `jti` is rejected at the
/// auth adapter, not only at the endpoint.
pub trait RevocationStore: Send + Sync {
    fn revoke(
        &self,
        jti: &str,
        ttl: Duration,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;
    fn is_revoked(&self, jti: &str) -> impl Future<Output = Result<bool, AuthError>> + Send;
}
