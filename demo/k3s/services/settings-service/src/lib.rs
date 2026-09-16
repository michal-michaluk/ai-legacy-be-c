//! Library root — declares each slice and re-exports its safe public surface.
//!
//! Each slice's `mod.rs` re-exports only its **ports + router** (never model
//! internals). Cross-context access goes through ports / injected values, never
//! direct module imports.

pub mod auth;
pub mod common;
pub mod notify;
pub mod router;
pub mod settings;
pub mod state;

// Shared DB-backed test support: starts a `postgres:16-alpine` testcontainer
// and exports `DATABASE_URL` via a `#[ctor]` BEFORE the test harness runs any
// `#[sqlx::test]`. Only compiled for lib unit tests when `pg-integration` is
// enabled (the default `cargo test --lib` is DB-less). The same source file is
// reused directly by `tests/backward_compat.rs` (`mod support;`).
#[cfg(all(test, feature = "pg-integration"))]
#[path = "../tests/support/mod.rs"]
mod pg_support;

#[cfg(test)]
pub mod contracts;

pub use state::AppState;
