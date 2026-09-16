//! Backward-compatibility test (be-arch §11, deterministic gate E).
//!
//! Proves the **forward path of a migration never breaks existing rows**: we
//! simulate the *prior* schema (the `settings` table exactly as `0001` left it,
//! WITHOUT the later `created_at` column), seed rows written under that prior
//! shape, then apply the real forward migration SQL (`0002_add_created_at.sql`)
//! and assert the rows are still readable and get the correct default for the
//! new column while keeping their existing `version`.
//!
//! Uses `#[sqlx::test]` against a throwaway Postgres so it needs the real
//! migrations. The Postgres instance is a disposable testcontainer started by
//! the shared support module (see `tests/support/mod.rs`); its `#[ctor]`
//! exports `DATABASE_URL` before the harness runs, so no external DB is needed.
//! SQL is read from the actual migration file — never hand-copied — so the test
//! cannot drift from the migration it verifies.

mod support;

use sqlx::PgPool;

/// The `settings` table as it existed after `0001_create_settings.sql` only
/// (i.e. the PRIOR schema, before `0002` added `created_at`).
const PRIOR_SCHEMA: &str = r#"
CREATE TABLE settings (
    id         BIGSERIAL   PRIMARY KEY,
    tenant     TEXT        NOT NULL,
    "user"     TEXT,
    key        TEXT        NOT NULL,
    value      JSONB       NOT NULL,
    version    INTEGER     NOT NULL DEFAULT 1,
    updated_at BIGINT      NOT NULL
);
CREATE UNIQUE INDEX ux_settings_identity ON settings (tenant, COALESCE("user", ''), key);
CREATE INDEX idx_settings_tenant_key ON settings (tenant, key);
"#;

#[sqlx::test(migrations = false)]
async fn forward_migration_keeps_prior_schema_rows_readable(pool: PgPool) {
    // 1. Build the PRIOR schema (no `created_at` column) and seed rows the way
    //    the app would have under `0001` (including a non-default `version`).
    sqlx::raw_sql(PRIOR_SCHEMA).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO settings (tenant, \"user\", key, value, version, updated_at) \
         VALUES ('acme', NULL, 'theme', '{\"color\":\"dark\"}'::jsonb, 3, 1000)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO settings (tenant, \"user\", key, value, version, updated_at) \
         VALUES ('acme', 'bob', 'locale', '{\"lang\":\"en\"}'::jsonb, 1, 2000)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // 2. Apply the REAL forward migration (0002) by executing the migration
    //    file verbatim.
    let mig2 = std::fs::read_to_string("migrations/0002_add_created_at.sql")
        .expect("0002 migration exists");
    sqlx::query(&mig2).execute(&pool).await.unwrap();

    // 3. The existing rows must survive, keep their `version`, and get the
    //    correct default (`NULL`) for the newly-added column.
    let (tenant, key, version, created_at): (String, String, i32, Option<i64>) = sqlx::query_as(
        "SELECT tenant, key, version, created_at FROM settings \
             WHERE tenant = 'acme' AND \"user\" IS NULL AND key = 'theme'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(tenant, "acme");
    assert_eq!(key, "theme");
    assert_eq!(version, 3, "prior row version must be preserved");
    assert_eq!(
        created_at, None,
        "new column defaults to NULL for prior rows"
    );

    // And the tenant-scoped query must still find exactly the two seeded rows.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE tenant = 'acme'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}
