//! `auth` (identity) slice — the authenticated principal and token-to-principal
//! validation.
//!
//! Re-exports its ports + the shared `Authenticated` extractor. The `settings`
//! slice uses only `auth::model` (Authority / Ownership) and the injected
//! `Authority` value.

mod auth;
mod auth_port;
mod error;
mod jwt;
mod model;
mod rest;
mod revocation;

pub use auth::AuthService;
pub use auth_port::{JwtPrincipal, JwtValidator, RevocationStore};
pub use error::AuthError;
pub use jwt::JwtAuthAdapter;
pub use model::{Authority, Ownership, Role, Tenant, UserId};
pub use rest::{logout, require_auth};
pub use revocation::InMemoryRevocationStore;
