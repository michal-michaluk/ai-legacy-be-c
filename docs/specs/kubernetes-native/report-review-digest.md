# Review report — PR/Branch digest
- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: /Users/michal/.agents/skills/review/SKILL.md §2 (descriptive digest)
- Evidence base: `git status --short`, `git diff --stat` in both repos; read of `src/otel_metrics.c`, `src/otel_metrics.h`, changed `src/*`, `test/**`, `docs/specs/kubernetes-native/*`, `demo/**`, `.agents/skills/run-k3s/**`

## Verdict
**Large / mixed-concerns** change: one real broker feature (OTLP metrics export under `WITH_OTEL`, default OFF) buried in a ~120-file uncommitted set that also contains a trace-context test suite, spec/doc updates, and a full k3s demo harness. Feature itself is bounded and well-gated; the packaging into a single branch is the main risk to reviewability.

## What it's about
Adds compile-time opt-in OpenTelemetry OTLP/HTTP *metrics* export to the Mosquitto broker (`WITH_OTEL`, default OFF), sourced from the existing `$SYS` metric set, plus documentation/conformance tests for MQTT5 trace-context carry (no broker code — already generic). Ships a local k3s + Rust demo to prove it end-to-end.

## Size
| Repo | Tracked | Untracked | Insertions/deletions |
|---|---|---|---|
| `mosquitto/` (submodule) | 15 files | ~22 files (incl. `otel_metrics.c` 822, `.h` 139, 13 test py ~1260) | +239 / −19 (tracked) |
| workspace root | 4 files (incl. `.gitignore`, `spec.md`, submodule pointer) | ~82 files (demo ~6927 LOC, run-k3s 703, docs 279) | +96 / −3 |

Classification: **Large** (≥20 files / ≥800 lines) and **mixed unrelated concerns** (broker C feature + Python E2E + spec markdown + Rust demo + cluster manifests).

## Complexity
Conceptually moderate; the risky spots, ranked:
- **New concurrency** — dedicated `pthread` export thread + `_Atomic` metric storage (`src/otel_metrics.c:625`, `src/sys_tree.h:118-130`). Correctness depends on the event loop being the sole *non-atomic* writer.
- **Cross-cutting build changes** — two build systems kept in correspondence (`config.mk`, `make/broker.mk`, `src/CMakeLists.txt`, `src/Makefile`), with a fail-fast `WITH_OTEL requires WITH_SYS_TREE` gate (`make/broker.mk:37`, `src/CMakeLists.txt:183`).
- **Monolithic config parser** — 4 new keys land inside `config__read_file_core` (`src/conf.c:1003-2974`, ~1972 lines, ~172 `else if` branches; new keys at `:2345-2372`).
- **Config ownership move** — `config__copy` transfers `otel` ownership via struct-copy + zeroing (`src/conf.c:746-751`); reload only warns (`otel_metrics__reload`).
- **No public C API/ABI change** — all new symbols are `#ifdef WITH_OTEL` internal (`src/otel_metrics.h`).

## What changed (grouped by area)
- **Broker feature (metrics export):** `src/otel_metrics.c/.h` (new), `src/sys_tree.c/.h` (metric source + atomics), `src/mosquitto.c:346,566` (stop/start + feature report), `src/mosquitto_broker_internal.h`, `src/conf.c` (keys), `src/http_api.c` (uses shared `struct metric`).
- **Build / feature flag:** `config.mk`, `make/broker.mk`, `src/CMakeLists.txt`, `src/Makefile`, `.gitignore`.
- **Tests:** `test/unit/otel/*` (CUnit encoder/classification table), `test/broker/24-otel-otlp-metrics.py` (G-C2.1–5 vs real collector), `test/broker/25-trace-context-*.py` (11) + `trace_context_helper.py` (G-C3.1–5), `test/otel/{collector.yaml,README.md,zero-overhead.sh}`; registrations in `test/broker/{Makefile,CMakeLists.txt,test.py}` and `test/unit/CMakeLists.txt`.
- **Spec / docs:** `docs/specs/kubernetes-native/spec.md` (§5.3, §5.4 gates G-C3.x/G-C4.x), new `trace-context-convention.md`, `validation-report.md`.
- **Demo harness (workspace):** `demo/` (k3s manifests, `mosquitto-otel.Dockerfile`, Rust `settings-service` copy + `notify/` MQTT-trace slice, recording scripts), `.agents/skills/run-k3s/`.

## Good-practices check vs `docs/arch/*.md`
| Doc | Expectation | Status |
|---|---|---|
| `feature-flags.md` | Default conservative, deps discovered only when enabled, fail configuration clearly, disabled build valid, capability report, tests for both paths | **Met** — `WITH_OTEL=no` default; `find_package(CURL REQUIRED)` inside `if(WITH_OTEL)`; `FATAL_ERROR` when `WITH_SYS_TREE` off; `report_features()` line (`src/mosquitto.c:331-333`); `zero-overhead.sh` asserts OFF/ON symbol+dependency delta |
| `memory-management.md` | Use allocator wrappers, check allocations, reverse-order cleanup, centralized destroy | **Met** — `mosquitto_malloc/realloc/strdup` throughout; `otel_metrics__config_cleanup` centralizes; partial-init cleanup in `otel__config_copy`/`start` |
| `error-handling.md` | Explicit errors, don't turn failure into success | **Mostly met** — invalid intervals/timeouts rejected (`src/conf.c:2351-2371`); export failures mapped+logged (`otel_metrics__classify`) |
| `logging.md` | Single choke point, no secrets logged | **Met** — `log__printf`; endpoint userinfo stripped via `otel__sanitize_endpoint` (`src/otel_metrics.c:652`) |
| `lifecycle-management.md` | Deterministic init/stop ordering | **Met** — start after `sys_tree__init()`, join in `post_shutdown_cleanup` |
| No secrets / debug code | — | **Met** — no tokens committed; `"Bearer test"` only in test harness |
| `docs/test/*` | Tests declare feature requirements | **Met** — `mosq_test.require_features(["WITH_OTEL"])` (`test/broker/24-otel-otlp-metrics.py:38`) |

## Open questions / unverified
- Digest cannot confirm the branch intent maps to the ticket trail referenced only in `spec.md` (issues #12–#21); no PR exists to read.
- `demo/k3s/services/settings-service/` is described in `validation-report.md:14` as a copy of a reference service — whether all copied files are intentionally in scope is unverified.
- `validation-report.md` F1 documents a pre-existing `custom_install` bug (`mosquitto/CMakeLists.txt:101-105`) left unfixed; out of this change's diff.
