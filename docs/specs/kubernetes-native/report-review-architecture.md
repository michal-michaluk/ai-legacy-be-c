# Review report — Architecture

- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: followed `/Users/michal/.agents/skills/review/references/review-arch-instructions.md` (no ArchUnit for C); evaluated conformance against `docs/arch/{code-structure,broker-runtime,feature-flags,persistence-and-state,public-api-and-library,memory-management,lifecycle-management,protocol-and-endpoints,logging}.md`.
- Evidence base: `git diff`/`git status` in `mosquitto/`; reads of `src/otel_metrics.c/.h`, `src/sys_tree.c/.h`, `src/conf.c`, `src/mosquitto.c`, `src/mosquitto_broker_internal.h`, `src/http_api.c`, `src/loop.c`, `config.mk`, `make/broker.mk`, `src/CMakeLists.txt`, `src/Makefile`, `test/**`; `find`/`grep` for an arch checker and public-header usage.

## Verdict

Structurally sound: the exporter is broker-only, the flag wiring matches `feature-flags.md`, and the `http_api.c` refactor removes a dead duplicate rather than introducing divergence. The one substantive concern is that the export thread keeps running and emitting when `sys_interval == 0`, because all metric/uptime refreshes live inside the `$SYS`-gated branch.

## Findings

| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| F1 | Medium | Runtime coupling gap: the exporter thread starts unconditionally and exports, but `sys_tree__update()` (the only place `uptime_current` and the gauge/load metrics refresh) is called only when `sys_interval > 0`; `sys_tree__init()` also early-returns for `sys_interval == 0` without setting `start_time`. Result: with `sys_interval 0` the thread still runs and emits `mosquitto_broker_uptime=0` and never-refreshed gauges/loads. | `src/loop.c:205-207`; `src/sys_tree.c:170-175` (init early-return); `src/sys_tree.c:270` (only `uptime_current` write site); `src/mosquitto.c:566` (unconditional start) | Skip/disable `otel_metrics__start` when `config.sys_interval == 0`, or decouple uptime/gauge refresh from the `$SYS` publish cadence. |
| F2 | Low | Hidden module-global state (`otel_thread`, `otel_runtime_config`, `otel_service_version`, `otel_instance_id`, `otel_running/otel_stop`) instead of broker/config ownership. `broker-runtime.md` says "Do not add hidden global state when the state belongs to … broker configuration." Acceptable only because it is a process-wide singleton. | `src/otel_metrics.c:615-621` (thread + runtime snapshot globals), `src/otel_metrics.c:680` (entry point) | Document the singleton explicitly, or store the runtime snapshot behind the broker config as the code already does for `struct otel_config`. |
| F3 | Low | `otel_metrics__init()` is declared and defined empty and is never called anywhere; dead API surface. | `src/otel_metrics.h:135`; `src/otel_metrics.c:818`; no callers found (`grep -rn otel_metrics__init src/`) | Remove the declaration/definition. |
| F4 | Low | Make-based build compiles `otel_metrics.o` unconditionally (`WITH_OTEL=no` yields an empty TU), while CMake adds the source only inside the `WITH_OTEL` branch. Inconsistent with `feature-flags.md` ("Keep feature-specific includes and link libraries inside the feature branch"). Harmless (no deps pulled) but diverges. | `src/Makefile:76`; `src/CMakeLists.txt:183-190` | Guard the extra object in the make build with `ifeq ($(WITH_OTEL),yes)` or keep the CMake alignment documented. |
| F5 | Info | No deterministic architecture checker exists in the repo (no ArchUnit/N-gram/dependency-rule test, no arch job in `.github/workflows`). Arch conformance is doc-only here. | `find . -iname "*arch*"` (only vendored plugin dirs); `.github/workflows/` listing | Out of scope for this change; note for future. |

## Good practices / strengths

- Single-threaded ownership model respected: broker event loop remains the only writer; the export thread reads via lock-free atomics (`struct metric` `current`/`next` `_Atomic`, `metric_loads[].current` `_Atomic double`, `uptime_current` `_Atomic`). No locks on the hot path. `src/sys_tree.h:118-127`; `src/sys_tree.c:142-144`, `src/sys_tree.c:216-244`.
- `http_api.c` refactor is clean: it removes a duplicate `struct metric` definition and an unused `extern metrics[]` that were dead (no reference to `metrics`/`is_max` in the file before or after), and now reuses the single definition from `sys_tree.h`. No divergent copy remains. `src/http_api.c:36`; `git show HEAD:src/http_api.c | grep maps nothing`.
- Flag wiring follows `feature-flags.md` exactly: `option_env` default OFF, dependency discovery inside the branch, `FATAL_ERROR`/`$(error)` when `WITH_SYS_TREE` absent, definitions/link libs on the owning target only, capability reported at startup, and tests declared via `require_features(["WITH_OTEL"])`. `src/CMakeLists.txt:97,183-190`; `make/broker.mk:37-45`; `src/mosquitto.c:330-334`; `test/broker/24-otel-otlp-metrics.py:41`.
- New internal symbols stay in broker-private headers: `otel_metrics.h` and the new `sys_tree.h` accessors are under `src/`; no `include/`, `lib/`, `common/` changes. `git diff --name-only | grep -E '^(include|lib|common)/'` → NONE.
- Lifecycle ordering is correct: start after `sys_tree__init()` (metric source ready), stop in `post_shutdown_cleanup()` in reverse; start/stop idempotent. `src/mosquitto.c:558-567,342-346`; `src/otel_metrics.c:717-733`.
- Logging uses the shared logger with severity mapping and sanitizes credentials from the endpoint before logging. `src/otel_metrics.c:680-731` (severity/lifecycle), `src/otel_metrics.h:68-78` (credential comment), `src/otel_metrics.c:652-677` (`otel__sanitize_endpoint`).

## Open questions / unverified

- F1: not proven by a failing test — no test drives `sys_interval 0` with OTEL enabled (unverified behavior; the design header comment acknowledges `sys_interval != 0` as the emit state but no guard exists).
- `_Atomic double`/`_Atomic int64_t` lock-freedom is platform-dependent; on targets where libatomic falls back to a lock, the event-loop writer could contend. Not measured (unverified).
- Whether `feature-flags.md`'s "tests for its disabled behavior" is satisfied by the default (OFF) CI builds only; no dedicated negative test exists.
