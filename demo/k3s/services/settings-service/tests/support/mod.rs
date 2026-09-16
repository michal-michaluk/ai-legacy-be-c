//! Shared test support for DB-backed tests against a disposable `postgres:16`
//! testcontainer (testcontainers). Enabled by the `pg-integration` feature.
//!
//! `#[sqlx::test]` reads `DATABASE_URL` *fresh* (via `dotenvy::var`) inside every
//! `test_context()` call — it is NOT captured at compile time. So starting the
//! container and setting `DATABASE_URL` in a `#[ctor]` global (which runs
//! *before* the test harness main starts any test) guarantees every
//! `#[sqlx::test]` sees a live Postgres and can create/drop its throwaway
//! per-test database — with no race against the parallel test threads.
//!
//! This single module is referenced from BOTH DB-backed entry points:
//!   * `tests/backward_compat.rs` (`cargo test --test backward_compat`), and
//!   * the lib's `#[sqlx::test]` integration module via the `pg-integration`
//!     feature (`cargo test --features pg-integration --lib`) — see
//!     `src/lib.rs`'s `#[cfg(all(test, feature = "pg-integration"))]` module.
//!
//! The container is held for the process lifetime in a `OnceLock`; when the test
//! binary exits, testcontainers removes it.

use std::sync::OnceLock;

use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::Container;
use testcontainers_modules::testcontainers::runners::SyncRunner;
use testcontainers_modules::testcontainers::ImageExt;

/// Pin the Postgres major version used for DB-backed tests. The reference
/// example targets Postgres 16; change here to pin another.
const PG_TAG: &str = "16-alpine";

/// Holds the started container for the process lifetime. Dropping it (on
/// process exit) removes the container, so DB-backed tests never leak Postgres.
static HOLD: OnceLock<Container<Postgres>> = OnceLock::new();

/// Start (once) a `postgres:16-alpine` testcontainer and export `DATABASE_URL`
/// into the process environment. Returns the resolved connection URL. Safe to
/// call many times — the container is started exactly once per process.
///
/// Returns an error if the container fails to start or is not ready.
pub fn start_pg() -> Result<String, Box<dyn std::error::Error + 'static>> {
    let container = HOLD.get_or_init(|| {
        let node: Container<Postgres> = Postgres::default()
            .with_host_auth()
            .with_tag(PG_TAG)
            .start()
            .expect("failed to start postgres:16-alpine testcontainer");
        node
    });

    let host = container.get_host()?;
    let port = container.get_host_port_ipv4(5432)?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    // Export BEFORE any `#[sqlx::test]` runs. The `#[ctor]` below runs before
    // the test harness main, so there is no race with parallel test threads.
    std::env::set_var("DATABASE_URL", &url);
    Ok(url)
}

/// `#[ctor]` — runs before `main`, i.e. before the test harness spawns any test.
/// This is exactly what makes the dynamically-set `DATABASE_URL` visible to all
/// `#[sqlx::test]` tests regardless of scheduling.
#[ctor::ctor]
fn init() {
    if let Err(e) = start_pg() {
        eprintln!("[pg_support] failed to start test Postgres: {e}");
        std::process::exit(1);
    }
}
