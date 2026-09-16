//! Compile-time contract assertions (be-arch §11, gate D).
//!
//! These are NOT runtime tests: `static_assertions` proves the bound *at compile
//! time*, so a regression (e.g. removing `Send` from a domain type or a port so
//! `axum` handlers stop being thread-safe) fails the build with a clear message
//! before any test runs. This module only needs to type-check; it is compiled
//! under `#[cfg(test)]` (see `lib.rs`).
//!
//! What we assert:
//! - the domain model types are `Send + Sync + Clone + Debug` (they are moved
//!   into / shared by `axum` handlers and the tokio runtime);
//! - `SettingKey`/`SettingValue` are `Send + Sync` (VOs cross the repo port);
//! - the generic services `SettingsService<R>` / `AuthService<J, Rv>` are
//!   `Send + Sync` when their port args are (they are held in the shared
//!   `AppState`).

use static_assertions::{assert_impl_all, assert_not_impl_any};

use crate::auth::{Authority, Ownership, Role, Tenant, UserId};
use crate::common::pagination::{Pagination, PaginationError};
use crate::settings::{Setting, SettingKey, SettingSnapshot, SettingValue};

// Domain building blocks must be trivially shareable across the tokio runtime
// (Send + Sync) and cloneable/debuggable (they move into snapshots, adapter-local
// responses, and span attributes).
assert_impl_all!(Setting: Send, Sync, Clone, std::fmt::Debug);
assert_impl_all!(SettingKey: Send, Sync, Clone, std::fmt::Debug);
assert_impl_all!(SettingValue: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(SettingSnapshot: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(Authority: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(Ownership: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(Tenant: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(UserId: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(Role: Send, Sync, Clone, std::fmt::Debug, Eq);
assert_impl_all!(Pagination: Send, Sync, Clone, Copy, std::fmt::Debug, Eq);
assert_impl_all!(PaginationError: Send, Sync, Clone, Copy, std::fmt::Debug, Eq);

// A pagination request self-validates on construction; it must never be
// `Default` (no silent "no pagination" state).
assert_not_impl_any!(Pagination: Default);

// The generic services are `Send + Sync` when their ports are, which lets axum
// handlers (which require `Send` blanket bounds) hold them in state.
assert_impl_all!(std::sync::Arc<crate::settings::SettingsService<
    crate::settings::PgSettingsRepository,
>>: Send, Sync);
assert_impl_all!(std::sync::Arc<crate::auth::AuthService<
    crate::auth::JwtAuthAdapter,
    crate::auth::InMemoryRevocationStore,
>>: Send, Sync);
