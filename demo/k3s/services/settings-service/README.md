# microservice-rust example — `settings-service`

A compilable, tested reference microservice for the `microservice-rust` blueprint:
**axum + sqlx + Postgres**, sliced by bounded context, ports & adapters via
generics with native `async fn`, DDD aggregates/VOs, semantic error taxonomy,
JWT resource-server auth (JWKS + revocation), OTel tracing/metrics, optimistic
concurrency (`ETag`/`If-Match`), and cloud-agnostic k8s manifests.

## Layout (flat, business-named slices — no `domain/`/`infra`/`dto`)

```text
src/
├── lib.rs                 # module declarations + re-exports
├── main.rs                # composition root (only place concrete adapters are named)
├── state.rs               # AppState<R, J, Rv> router state
├── common/
│   ├── config.rs          # env-driven Config, fail-fast
│   ├── error.rs           # generic ApiError (imports NO context)
│   ├── pagination.rs      # Pagination value object
│   ├── rate_limit.rs      # fixed-window limiter (A9)
│   └── telemetry.rs       # tracing + OTel + Prometheus
├── auth/                  # identity context
│   ├── model.rs           # Authority / Ownership / Role / Tenant / UserId
│   ├── auth_port.rs       # JwtValidator, RevocationStore (generic, native async fn)
│   ├── auth.rs            # AuthService<J, Rv>
│   ├── jwt.rs             # JWKS resource-server adapter
│   ├── revocation.rs      # in-memory jti blacklist (A14)
│   ├── rest.rs            # require_auth middleware + /logout + From<AuthError>
│   └── error.rs           # AuthError + stable codes
└── settings/              # business context
    ├── model.rs           # Setting aggregate + SettingKey/SettingValue VOs + snapshot
    ├── settings.rs        # SettingsService<R: SettingRepository> + real-repo tests
    ├── settings_repository.rs # SettingRepository port (RPITIT + Send)
    ├── pg_settings.rs     # PgSettingsRepository (sqlx non-macro, optimistic locking)
    ├── rest.rs            # /settings handlers + SettingResponse + From<SettingsError>
    └── error.rs           # SettingsError + stable codes
```

## Commands (all passing)

```sh
cargo build                      # ✓
cargo test                       # ✓ 37 unit + 4 arch + 1 backward-compat (PG-backed)
cargo clippy -- -D warnings      # ✓
cargo fmt --check                # ✓
```

`cargo test` runs the real-repo integration tests via `#[sqlx::test]`, which
create a disposable Postgres database per test (apply migrations). Point
`DATABASE_URL` at a Postgres with `CREATEDB`:

```sh
docker run -d --name devpg -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16-alpine
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test
```

There is deliberately **no in-memory repository double**: the persistence and
optimistic-locking behaviour is proven on the real `PgSettingsRepository`
(see `hex-doubles.md`, which the example documents as an optional pattern).

## Migrations (as code — never at startup, D1/D11)

```sh
just migrate   # builds sqlx-cli locally + runs `migrate run`
```

`launchbadge/sqlx-cli` is retired/removed from Docker Hub, so the example builds
the `sqlx` CLI from source (`Dockerfile.sqlx-cli`) and runs it as a dedicated
image/Job — never in the app container (D1/D11).

## Quality gates (be-arch §11, `justfile`)

| Gate | `just` target | Tool |
|---|---|---|
| A · coverage floor (whole-crate ≥ 85%, by-design noise excluded) | `just coverage` | `cargo llvm-cov` (`--fail-under-lines`) |
| B · SCA | `just deny-advisories` / `deny-licenses` / `deny` | cargo-deny |
| C · secrets + IaC | `just trivy-secrets` / `trivy-iac` / `trivy` | Trivy |
| C · image quality | `just dockerfile-lint` | droast (mirrors microsandbox verify-gates.sh) |
| D · compile-time contracts | `just contracts` | `static_assertions` |
| E · backward-compatible migration | `just backward-compat` | `#[sqlx::test]` |
| F · architecture rules | `just architecture` | `tests/architecture_test.rs` |
| G · Pact provider verification | `just pact-verify` | `pact-verifier` |
| H · k3s + hurl e2e | `just deploy-local` / `e2e` / `e2e-full` | `k3s` + `hurl` |

See `docs/ci-cd.md` for the full matrix and `scripts/e2e.sh` for the k3s deploy.

## Conventions demonstrated

- **Vertical slices** (`code-structure`, `vertical-slices`): `settings` and
  `auth` are self-contained business slices; no layer-named top module.
- **Ports & adapters** (`ports-and-adapters`): `SettingsService<R: SettingRepository>`,
  `AuthService<J: JwtValidator, Rv: RevocationStore>`; generics, native `async fn`,
  no `Arc<dyn>` / `#[async_trait]`.
- **DDD** (`ddd-building-blocks`): `Setting` aggregate; `SettingKey`/`SettingValue`
  VOs; `SettingSnapshot` value object; domain is pure (no async, `now` injected).
- **Error taxonomy** (`error-taxonomy`): per-slice thiserror with stable codes
  (`settings:version_conflict`, `auth:unauthorized`, …); `ApiError` mapper with
  act/retry/bug buckets, `Retry-After`, opaque 500 (A10).
- **Security** (`security`): JWKS resource-server (iss+aud+sig), tenant from token,
  `Authority`/`Ownership`, roles User/Admin, default-deny middleware, server-side
  logout revocation (A14), rate limiting (A9), no password storage (A17).
- **Optimistic concurrency** (`be-arch §10`): `version` column, atomic
  `WHERE version = $n` + `RETURNING version`, `ETag`/`If-Match`, stale → 409.
- **Observability** (`observability`): `tenant`/`user`/`role` span attributes,
  OTel-native errors, `/metrics`, `/telemetry` proxy.
- **Deployment** (`deployment-requirements`): cloud-agnostic `k8s/`, migration
  step, one physical PG + per-service logical DB, OTel Collector → `traces.jsonl`.
