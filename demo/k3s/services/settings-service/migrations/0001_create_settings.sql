-- 0001_create_settings.sql
-- Tenant-scoped JSON config key -> value store.
-- `user` is NULL for a tenant-scoped (Admin-owned) entry; it can never be a
-- PRIMARY KEY member (Postgres forces PK columns NOT NULL, which would forbid
-- tenant-scoped rows), so identity is enforced by a functional unique index
-- that collapses NULL user to '' for uniqueness.
CREATE TABLE settings (
    id         BIGSERIAL   PRIMARY KEY,
    tenant     TEXT        NOT NULL,
    "user"     TEXT,                       -- NULL = tenant-scoped entry (Admin only)
    key        TEXT        NOT NULL,
    value      JSONB       NOT NULL,
    version    INTEGER     NOT NULL DEFAULT 1,
    updated_at BIGINT      NOT NULL
);

-- One row per (tenant, user-or-tenant-scope, key). NULL user maps to '' so a
-- tenant-scoped entry is unique while still allowing user-scoped rows.
CREATE UNIQUE INDEX ux_settings_identity ON settings (tenant, COALESCE("user", ''), key);

-- Fuzzy search over key + value::text benefits from a tenant-grain index.
CREATE INDEX idx_settings_tenant_key ON settings (tenant, key);
