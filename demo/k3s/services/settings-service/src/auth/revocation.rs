//! Server-side logout revocation store (A14).
//!
//! An **in-memory** blacklist of revoked session ids (`jti`), each with an
//! expiry equal to the token's remaining TTL. This is a port double that also
//! serves as the default local adapter — no Redis required to run locally. A
//! production deployment swaps in a Redis-backed store behind the same
//! `RevocationStore` port (adapters are replaceable, be-arch §7).

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::auth::auth_port::RevocationStore as RevocationStorePort;
use crate::auth::error::AuthError;

/// In-memory `jti -> expiry` blacklist implementing the `RevocationStore` port
/// with native `async fn` (no `#[async_trait]`).
#[derive(Default)]
pub struct InMemoryRevocationStore {
    inner: RwLock<HashMap<String, Instant>>,
}

impl InMemoryRevocationStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

impl RevocationStorePort for InMemoryRevocationStore {
    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), AuthError> {
        let expiry = if ttl.is_zero() {
            Instant::now()
        } else {
            Instant::now() + ttl
        };
        self.inner
            .write()
            .map_err(|_| AuthError::Internal)?
            .insert(jti.to_string(), expiry);
        Ok(())
    }

    async fn is_revoked(&self, jti: &str) -> Result<bool, AuthError> {
        let mut guard = self.inner.write().map_err(|_| AuthError::Internal)?;
        match guard.get(jti) {
            Some(&expiry) => {
                if expiry > Instant::now() {
                    Ok(true)
                } else {
                    guard.remove(jti);
                    Ok(false)
                }
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn revoke_then_is_revoked_until_expiry() {
        let store = InMemoryRevocationStore::new();
        assert!(!store.is_revoked("jti-1").await.unwrap());
        store
            .revoke("jti-1", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(store.is_revoked("jti-1").await.unwrap());
    }

    #[tokio::test]
    async fn not_revoked_after_ttl() {
        let store = InMemoryRevocationStore::new();
        store.revoke("jti-2", Duration::ZERO).await.unwrap();
        assert!(!store.is_revoked("jti-2").await.unwrap());
    }
}
