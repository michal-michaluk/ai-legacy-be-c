//! `AuthService` — the auth slice's primary port implementation, generic over
//! its two secondary ports (`JwtValidator`, `RevocationStore`). Two separate
//! generic params — not a bundle trait (ADR-003).

use tracing::instrument;

use crate::auth::auth_port::{JwtPrincipal, JwtValidator, RevocationStore};
use crate::auth::error::AuthError;
use crate::auth::model::Authority;

/// Two generic type params — the `auth` slice's ports are NOT bundled.
pub struct AuthService<J: JwtValidator, R: RevocationStore> {
    jwt: J,
    revocation: R,
}

impl<J: JwtValidator, R: RevocationStore> AuthService<J, R> {
    pub fn new(jwt: J, revocation: R) -> Self {
        Self { jwt, revocation }
    }

    /// Validate a bearer token (signature + iss + aud via `JwtValidator`) and
    /// reject a revoked `jti` (A14) **before** mapping it to `Authority`.
    #[instrument(skip(self, token))]
    pub async fn validate_token(&self, token: &str) -> Result<Authority, AuthError> {
        let principal = self.jwt.validate(token).await?;
        if self.revocation.is_revoked(principal.jti()).await? {
            return Err(AuthError::Revoked);
        }
        Ok(principal.authority().clone())
    }

    /// Validate the token and revoke its `jti` for the remaining TTL (A14).
    /// Reuse of the token afterwards is rejected at the auth adapter.
    #[instrument(skip(self, token))]
    pub async fn logout(&self, token: &str) -> Result<(), AuthError> {
        let principal: JwtPrincipal = self.jwt.validate(token).await?;
        self.revocation
            .revoke(principal.jti(), principal.remaining_ttl())
            .await?;
        Ok(())
    }
}
