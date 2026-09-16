# Review report — Backward Compatibility

- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: followed `/Users/michal/.agents/skills/review/references/review-backward-compatibility-instructions.md` (placeholder; applied its listed scope — API/contract/schema compatibility, semver/additive changes).
- Evidence base: `git diff --stat`/`git status` in `mosquitto/`; reads of `src/conf.c`, `src/sys_tree.c/.h`, `config.mk`, `make/broker.mk`, `src/CMakeLists.txt`, `src/Makefile`, `src/mosquitto.c`, `src/mosquitto_broker_internal.h`, `src/http_api.c`; `git diff --name-only` filtered for `include/`, `lib/`, `common/`.

## Verdict

Backward compatible: the public C API/ABI is untouched, all new behavior is additive and compile-time guarded by `WITH_OTEL` (default OFF), and the `WITH_OTEL=OFF` build path — including `$SYS` output — is byte-for-byte the pre-change path. Remaining notes are about internal-layout consistency, not external contracts.

## Findings

| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| B1 | Info | Public C API/ABI unchanged: no changes to installed headers or `libmosquitto`. | `git diff --name-only | grep -E '^(include|lib|common)/'` → NONE; `git diff --stat` lists only `src/**`, build files, `test/**`. | None. |
| B2 | Info | `struct metric` size/layout for the default `WITH_OTEL=OFF` build is unchanged (`int64_t current; int64_t next;`). The struct only moved from `sys_tree.c` to internal `sys_tree.h`; it is a broker-private (`WITH_SYS_TREE && WITH_BROKER`) structure, not an installed header, so not an ABI commitment. | `src/sys_tree.h:118-130`; `src/sys_tree.h:24-25` (guard); `src/http_api.c:36` (new consumer) | None. |
| B3 | Info | Under `WITH_OTEL` the same fields become `_Atomic int64_t` (same size on lock-free targets). Blast radius is confined to the broker TU set; no external consumer sees the layout. | `src/sys_tree.h:119-123`; `src/sys_tree.c:37-44` (`_Atomic double` for loads) | None. |
| B4 | Info | New `mosquitto.conf` keys are additive and compile-time guarded; when absent, existing config parses identically. `otlp_export_interval`/`otlp_timeout` reject `< 1` with an explicit error rather than silently defaulting. | `src/conf.c:2345-2372` (`otlp_endpoint`, `otlp_export_interval`, `otlp_headers`, `otlp_timeout`); defaults `src/conf.c:353`; cleanup `src/conf.c:404,747-749` | None. |
| B5 | Info | Default is OFF in every build system. | `config.mk:155` (`WITH_OTEL=no`); `src/CMakeLists.txt:97` (`option_env(... OFF)`) | None. |
| B6 | Info | `WITH_OTEL=OFF` behavior is identical incl. `$SYS`: all exporter code is inside `#ifdef WITH_OTEL` (`otel_metrics.c` wraps the whole file; `sys_tree.c` accessors, `uptime_current`, and atomicity are conditional); the `$SYS` publish block is unchanged. | `src/otel_metrics.c:21,822` (whole-file guard); `src/sys_tree.c:144,218-248,270`; unchanged `sys_tree__update` emit loop `src/sys_tree.c:259-333` | None. |
| B7 | Low | `struct mosquitto__config` gains an `otel` member only under `WITH_OTEL`; this is an internal header, so no ABI impact, but a build with mismatched `WITH_OTEL` between objects would see a different struct layout (same class of caveat as all existing feature flags). | `src/mosquitto_broker_internal.h:44-45,374-376` | None required; ensure the flag is defined consistently for all broker TUs (already done via target/Makefile flags). |
| B8 | Low | Make build always links/compiles `otel_metrics.o` even with `WITH_OTEL=no`; it resolves to an empty TU so no new runtime/link dependency is introduced (`-lcurl`/`-pthread` added only when enabled). | `src/Makefile:76`; `make/broker.mk:37-45`; guard `src/otel_metrics.c:19-20` | Optional: guard the object behind `ifeq ($(WITH_OTEL),yes)` for symmetry with CMake. |

## Good practices / strengths

- No shared file between broker and `libmosquitto` was touched; the `http_api.c` change only deletes broker-private dead code and adds an include of an internal header. `git diff --name-only` → no `lib/`/`include/`/`common/` entries.
- Feature contract is enforced defensively: `WITH_OTEL` requires `WITH_SYS_TREE` at both build systems, so the metric source can never be silently absent. `make/broker.mk:38-40`; `src/CMakeLists.txt:184-186`.
- Config ownership on reload is explicit (move semantics + re-init defaults), preventing double-free/leak of the previous endpoint/headers. `src/conf.c:747-750`; defaults restored `src/conf.c:353`.
- Capability is reported in startup diagnostics, so an operator can tell OFF from ON. `src/mosquitto.c:330-334`.

## Open questions / unverified

- No test exercises a `WITH_OTEL=OFF` broker reading a config file that contains `otlp_*` keys; expected behavior is an "unknown configuration variable" warning (unverified, but the tokens fall through the guarded branches to the existing unknown-token handler).
- ABI equivalence of `_Atomic int64_t` vs `int64_t` size is asserted only by C11 semantics; not measured on each target (internal only, so no external exposure).
- Whether the wiring duplicates `WITH_OTEL` across `config.mk` and `CMakeLists.txt` with a documented correspondence (`feature-flags.md` asks for this when a flag lives in two build systems) — not verified for this flag specifically.
