//! PgSQL persistence adapter (`PgSettingsRepository`) implementing the
//! `SettingRepository` port, plus the in-memory hex-double.
//!
//! Uses the **non-macro** sqlx API (`query_as` / `query`) — builds and tests
//! with no live database and no committed `.sqlx/` offline metadata. SQL stays
//! visible and parameterized (`$N` binds, never string interpolation). The
//! adapter record maps <-> the domain aggregate; the aggregate never sees a
//! `FromRow` type.
//!
//! Writes are **conditionally atomic** (be-arch §10): `version = version + 1`
//! computed server-side, and the write only matches `WHERE version = $expected`
//! so a stale precondition is detected atomically.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::postgres::PgPoolOptions;
use sqlx::FromRow;
use sqlx::PgPool;

use crate::auth::{Authority, Ownership, Role, Tenant, UserId};
use crate::common::pagination::Pagination;
use crate::settings::error::SettingsError;
use crate::settings::model::{Setting, SettingKey, SettingValue};
use crate::settings::settings_repository::{SearchPage, SettingRepository};

/// A row as stored. `value` is `jsonb`; `updated_at` is **unix milliseconds**;
/// `user` is nullable (tenant-scoped when `NULL`).
#[derive(FromRow)]
struct SettingRecord {
    tenant: String,
    user: Option<String>,
    key: String,
    value: sqlx::types::Json<serde_json::Value>,
    version: i32,
    updated_at: i64,
}

impl SettingRecord {
    fn into_domain(self) -> Result<Setting, SettingsError> {
        let key = SettingKey::new(&self.key)?;
        let value = SettingValue::new(self.value.0)?;
        let tenant = Tenant::new(&self.tenant);
        let ownership = match self.user {
            Some(u) if !u.is_empty() => Ownership::for_user(tenant, UserId::new(&u)),
            _ => Ownership::for_tenant(tenant),
        };
        Setting::restore(
            key,
            value,
            ownership,
            self.version.max(1) as u64,
            millis_to_instant(self.updated_at),
        )
    }
}

/// A row of the search CTE output: the page fields are nullable because of the
/// `LEFT JOIN page p ON TRUE` (they are NULL exactly when the page is empty,
/// so `total` is still returned); `total` is always set (COUNT(*)::BIGINT).
#[derive(FromRow)]
struct SearchRow {
    tenant: Option<String>,
    user: Option<String>,
    key: Option<String>,
    value: Option<sqlx::types::Json<serde_json::Value>>,
    version: Option<i32>,
    updated_at: Option<i64>,
    total: i64,
}

/// sqlx persistence adapter. Owns the `PgPool`; transactions are an adapter
/// detail scoped to one aggregate.
pub struct PgSettingsRepository {
    pool: PgPool,
}

impl PgSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Build a pool against `database_url` without connecting (lazy), for
    /// wiring at the composition root.
    pub fn from_url(database_url: &str) -> Result<Self, SettingsError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_lazy(database_url)
            .map_err(map_pg_err)?;
        Ok(Self { pool })
    }

    async fn exists(&self, key: &SettingKey, ownership: &Ownership) -> Result<bool, SettingsError> {
        let row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM settings WHERE tenant = $1 AND COALESCE(\"user\", '') = COALESCE($2, '') AND key = $3",
        )
        .bind(ownership.tenant().as_str())
        .bind(ownership.user().map(|u| u.as_str()))
        .bind(key.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_pg_err)?;
        Ok(row > 0)
    }
}

impl SettingRepository for PgSettingsRepository {
    async fn find(&self, key: &SettingKey, tenant: &Tenant) -> Result<Vec<Setting>, SettingsError> {
        let rows = sqlx::query_as::<_, SettingRecord>(
            "SELECT tenant, \"user\", key, value, version, updated_at \
             FROM settings WHERE tenant = $1 AND key = $2 ORDER BY \"user\" NULLS FIRST",
        )
        .bind(tenant.as_str())
        .bind(key.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_err)?;
        rows.into_iter().map(SettingRecord::into_domain).collect()
    }

    async fn insert(&self, setting: &Setting) -> Result<Setting, SettingsError> {
        let rec = sqlx::query_as::<_, SettingRecord>(
            "INSERT INTO settings (tenant, \"user\", key, value, version, updated_at) \
             VALUES ($1, $2, $3, $4, 1, $5) \
             ON CONFLICT (tenant, COALESCE(\"user\", ''), key) DO NOTHING \
             RETURNING tenant, \"user\", key, value, version, updated_at",
        )
        .bind(setting.ownership().tenant().as_str())
        .bind(setting.ownership().user().map(|u| u.as_str()))
        .bind(setting.key().as_str())
        .bind(sqlx::types::Json(setting.value().as_value().clone()))
        .bind(instant_to_millis(setting.updated_at()))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_err)?;
        match rec {
            Some(r) => r.into_domain(),
            None => Err(SettingsError::Conflict),
        }
    }

    async fn update(
        &self,
        setting: &Setting,
        expected_version: u64,
    ) -> Result<Setting, SettingsError> {
        let updated = sqlx::query_as::<_, SettingRecord>(
            "UPDATE settings \
             SET value = $1, updated_at = $2, version = version + 1 \
             WHERE tenant = $3 AND COALESCE(\"user\", '') = COALESCE($4, '') AND key = $5 AND version = $6 \
             RETURNING tenant, \"user\", key, value, version, updated_at",
        )
        .bind(sqlx::types::Json(setting.value().as_value().clone()))
        .bind(instant_to_millis(setting.updated_at()))
        .bind(setting.ownership().tenant().as_str())
        .bind(setting.ownership().user().map(|u| u.as_str()))
        .bind(setting.key().as_str())
        .bind(expected_version as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_err)?;
        match updated {
            Some(r) => r.into_domain(),
            None => {
                if self.exists(setting.key(), setting.ownership()).await? {
                    Err(SettingsError::VersionConflict)
                } else {
                    Err(SettingsError::NotFound)
                }
            }
        }
    }

    async fn delete(
        &self,
        key: &SettingKey,
        ownership: &Ownership,
        expected_version: u64,
    ) -> Result<(), SettingsError> {
        let deleted = sqlx::query_as::<_, SettingRecord>(
            "DELETE FROM settings \
             WHERE tenant = $1 AND COALESCE(\"user\", '') = COALESCE($2, '') AND key = $3 AND version = $4 \
             RETURNING tenant, \"user\", key, value, version, updated_at",
        )
        .bind(ownership.tenant().as_str())
        .bind(ownership.user().map(|u| u.as_str()))
        .bind(key.as_str())
        .bind(expected_version as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_err)?;
        match deleted {
            Some(_) => Ok(()),
            None => {
                if self.exists(key, ownership).await? {
                    Err(SettingsError::VersionConflict)
                } else {
                    Err(SettingsError::NotFound)
                }
            }
        }
    }

    async fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> Result<SearchPage, SettingsError> {
        let pattern = format!("%{}%", query);
        // The ownership/read-permission predicate is bound into the `visible`
        // WHERE so LIMIT/OFFSET AND COUNT(*) count only rows the caller may see
        // (D1). $3 = the caller's own user id (User and Admin both see own rows);
        // $4 = is_admin; only an Admin additionally sees tenant-scoped rows
        // ("user" IS NULL). No role ever sees another user's rows.
        //
        // Single-statement CTE: `visible` = the FULLY filtered set; `page` =
        // the requested slice; `total` = the count of that SAME visible set.
        // `FROM total t LEFT JOIN page p ON TRUE` yields at least one row even
        // when the page is empty (out-of-range page number), so `total` is
        // always returned. Rows with a NULL `key` are the empty-page sentinel
        // and are dropped by the adapter.
        let is_admin = authority.role() == Role::Admin;
        let rows = sqlx::query_as::<_, SearchRow>(
            "WITH visible AS (
                 SELECT tenant, \"user\", key, value, version, updated_at
                 FROM settings
                 WHERE tenant = $1
                   AND (key ILIKE $2 OR value::text ILIKE $2)
                   AND (\"user\" = $3 OR ($4 AND \"user\" IS NULL))
             ),
             page AS (
                 SELECT * FROM visible ORDER BY key LIMIT $5 OFFSET $6
             ),
             total AS (
                 SELECT COUNT(*)::BIGINT AS total FROM visible
             )
             SELECT p.tenant, p.\"user\", p.key, p.value, p.version, p.updated_at, t.total
             FROM total t
             LEFT JOIN page p ON TRUE
             ORDER BY p.key",
        )
        .bind(authority.tenant().as_str())
        .bind(&pattern)
        .bind(authority.user().as_str())
        .bind(is_admin)
        .bind(pagination.limit() as i64)
        .bind(pagination.offset() as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_err)?;

        let mut page_rows = Vec::new();
        let mut total = 0u64;
        for row in rows {
            // Every row carries the same `total`; overwrite so the last one wins
            // (identical value).
            total = row.total.max(0) as u64;
            // A NULL `key` marks the empty-page sentinel row (page is empty).
            if let Some(key) = row.key {
                page_rows.push(
                    SettingRecord {
                        tenant: row.tenant.expect("non-null page row has tenant"),
                        user: row.user,
                        key,
                        value: row.value.expect("non-null page row has value"),
                        version: row.version.expect("non-null page row has version"),
                        updated_at: row.updated_at.expect("non-null page row has updated_at"),
                    }
                    .into_domain()?,
                );
            }
        }
        Ok(SearchPage {
            rows: page_rows,
            total,
        })
    }

    async fn ping(&self) -> Result<(), SettingsError> {
        // Round-trip `SELECT 1` to prove the backing store is reachable for the
        // `/readyz` readiness probe. `1::BIGINT` so it matches the Rust `i64`
        // scalar type (plain `1` returns INT4, which sqlx rejects).
        sqlx::query_scalar::<_, i64>("SELECT 1::BIGINT")
            .fetch_one(&self.pool)
            .await
            .map(|_| ())
            .map_err(map_pg_err)
    }
}

/// Classify a sqlx error: connection / pool timeouts are transient (`503`),
/// everything else is opaque (`500`).
fn map_pg_err(e: sqlx::Error) -> SettingsError {
    match &e {
        sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) => {
            tracing::warn!(error = %e, "database unavailable");
            SettingsError::Unavailable
        }
        _ => {
            tracing::error!(error = %e, "database error");
            SettingsError::Internal
        }
    }
}

/// The single anchored ephemeral->wall-clock base, captured **once** per process
/// via `OnceLock`. Both `instant_to_millis` and `millis_to_instant` derive from
/// this same `(Instant, epoch_millis)` pair, so an `Instant` round-trips to the
/// exact epoch-millis it was built from (no per-call wall-clock drift).
pub(crate) fn base() -> (std::time::Instant, u64) {
    use std::sync::OnceLock;
    static BASE: OnceLock<(std::time::Instant, u64)> = OnceLock::new();
    *BASE.get_or_init(|| {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        (std::time::Instant::now(), now_ms)
    })
}
/// `Instant` -> epoch-millis (`i64`), anchored to `base()`.
pub(crate) fn instant_to_millis(i: std::time::Instant) -> i64 {
    let (base_inst, base_ms) = base();
    (base_ms as i64) + i.saturating_duration_since(base_inst).as_millis() as i64
}
/// epoch-millis (`i64`) -> `Instant`, anchored to `base()`. Public so the pact
/// harness can seed a row from a fixed epoch and get the same `Instant` back
/// (deterministic RFC-3339 `updated_at`).
pub fn millis_to_instant(m: i64) -> std::time::Instant {
    let (base_inst, base_ms) = base();
    base_inst + Duration::from_millis(m.saturating_sub(base_ms as i64).max(0) as u64)
}
