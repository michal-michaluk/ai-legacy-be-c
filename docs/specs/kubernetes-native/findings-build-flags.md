# Findings — compile-time feature flags (Q5)

Evidence for the opt-in build capability. Paths relative to `mosquitto/`.

## Build system

- **CMake is primary**; plain makefiles "will be removed in version 3.0" (`README-compiling.md:32-38`).
- `config.mk` is the legacy GNU-make config, `include`d by the top-level `Makefile:1` and sub-makefiles (`src/Makefile:2`); overridable as `make WITH_TLS=no` (`config.mk:10-11`).
- The two systems are **independent parallel definitions** of the same feature set (no shared source of truth).
- CMake helper: `option_env(name desc value)` — wraps `option()` + records into `TEST_ENV_VARS` (`CMakeLists.txt:43-48`).
- Definition mechanisms: `add_definitions("-DWITH_X")` (global, `CMakeLists.txt:24,137-175`), `target_compile_definitions(target PRIVATE "WITH_X")` (scoped, `src/CMakeLists.txt:135,139,…`), `add_compile_definitions(...)` (`CMakeLists.txt:86`), `INTERFACE` propagate (`client/CMakeLists.txt:17,21`).
- Make mechanism: `make/<module>.mk` reads `config.mk` and appends `-DWITH_X` to `LOCAL_CPPFLAGS`.
- **No `WITHOUT_*` macros exist** — all toggling is presence/absence of `WITH_*`.

## Existing options (selected)

| config.mk (line) | CMake option (file:line) | Definition | Default |
|---|---|---|---|
| `WITH_TLS:=yes` (24) | `WITH_TLS` `CMakeLists.txt:72` | `WITH_TLS` | yes / ON |
| `WITH_BRIDGE:=yes` (38) | `INC_BRIDGE_SUPPORT` `src/CMakeLists.txt:90` | `WITH_BRIDGE` | yes / ON |
| `WITH_PERSISTENCE:=yes` (44) | `WITH_PERSISTENCE` `src/CMakeLists.txt:97` | `WITH_PERSISTENCE` | yes / ON |
| `WITH_MEMORY_TRACKING:=yes` (49) | `INC_MEMTRACK` `CMakeLists.txt:58` | `WITH_MEMORY_TRACKING` | yes / ON |
| `WITH_SYS_TREE:=yes` (64) | `WITH_SYS_TREE` `src/CMakeLists.txt:99` | `WITH_SYS_TREE` | yes / ON |
| `WITH_SYSTEMD:=no` (70) | `WITH_SYSTEMD` `src/CMakeLists.txt:205` | `WITH_SYSTEMD` | no / OFF |
| `WITH_WEBSOCKETS:=yes` (79) | `WITH_WEBSOCKETS` `CMakeLists.txt:76` | `WITH_WEBSOCKETS=WS_IS_{BUILTIN,LWS}` | yes / ON |
| `WITH_SOCKS:=yes` (85) | `WITH_SOCKS` `CMakeLists.txt:68` | `WITH_SOCKS` | yes / ON |
| `WITH_CONTROL:=yes` (117) | `WITH_CONTROL` `src/CMakeLists.txt:94` | `WITH_CONTROL` | yes / ON |
| `WITH_HTTP_API=yes` (151) | `WITH_HTTP_API` `src/CMakeLists.txt:95` | `WITH_HTTP_API` | yes / ON |
| `WITH_FUZZING=no` (142) | `WITH_FUZZING` `CMakeLists.txt:62` | `WITH_FUZZING` | no / OFF |
| `WITH_OLD_KEEPALIVE=no` (128) | `WITH_OLD_KEEPALIVE` `src/CMakeLists.txt:96` | `WITH_OLD_KEEPALIVE` | no / OFF |
| `WITH_XTREPORT=no` (124) | `WITH_XTREPORT` `src/CMakeLists.txt:100` | `WITH_XTREPORT` | no / OFF |

Name divergences: `WITH_MEMORY_TRACKING`↔`INC_MEMTRACK`; `WITH_EDITLINE`↔`WITH_CTRL_SHELL`;
`WITH_SQLITE`↔`WITH_PLUGIN_PERSIST_SQLITE`. `option_env(WITH_PERSISTENCE …)` is duplicated
(`src/CMakeLists.txt:97-98`).

## End-to-end pattern — `WITH_HTTP_API` (closest analog: optional HTTP endpoint)

1. **config.mk** `:150-151` — `WITH_HTTP_API=yes`.
2. **make/broker.mk** `:30-33` — `ifeq ($(WITH_HTTP_API),yes)` → `LOCAL_CPPFLAGS+=-DWITH_HTTP_API`, `LOCAL_LDADD+=-lmicrohttpd`. Source in `src/Makefile:47` always; guarded internally.
3. **CMake option** `src/CMakeLists.txt:95` — `option_env(WITH_HTTP_API "…" ON)`.
4. **CMake wiring** `src/CMakeLists.txt:148-166` — `pkg_check_modules`/`find_library`, `target_sources`, `target_compile_definitions`, `target_link_libraries`; warns and disables if lib missing.
5. **Source guard** `src/http_api.c:19-21` — `#include "config.h"` then `#ifdef WITH_HTTP_API`.
6. **Wiring** — listener protocol parse `src/conf.c:2456-2462`; struct field `mosquitto_broker_internal.h:285-287`; start/stop `src/listeners.c:207,245,297,329`.
7. **Feature report** — `report_features()` `src/mosquitto.c:303-331`.

## Recipe — add a new opt-in flag

1. `config.mk` — add `WITH_X:=no` with a comment block (style of `:150-151`).
2. `make/broker.mk` — `ifeq ($(WITH_X),yes)` → `LOCAL_CPPFLAGS+=-DWITH_X` (+ `-l…`), mirror `:30-33`. Add new `.c` to `OBJS` in `src/Makefile` (guarded internally).
3. `src/CMakeLists.txt` — `option_env(WITH_X "…" OFF)` near `:90-100`; definition block near `:148-166` (`target_sources` + `target_compile_definitions(mosquitto PRIVATE …)`; `find_package`/`pkg_check_modules` + link for deps, warn+disable if absent).
4. `config.h` — only if the macro must derive another at include time (as `WITH_TLS`→`FINAL_WITH_TLS_PSK` `:67-71`).
5. New source file — `#include "config.h"` + `#ifdef WITH_X` around the whole implementation.
6. Wire into lifecycle (`src/listeners.c`, `src/conf.c`) behind `#ifdef`.
7. `report_features()` `src/mosquitto.c:303-331`.
8. Docs — `README.md:83-105`, `README-compiling.md:1-16`, `ChangeLog.txt`. Man pages do **not** document build options.

## Defaults / independence

- Most features default **opt-out** (on); security/optional integrations default **off**.
- **Recommended for the new flags: default OFF** (matches "restrictive environments" header `config.mk:1-9`).
- Dependencies enforced ad hoc only (`WITH_TLS_PSK`⊂`WITH_TLS` `config.mk:196-201`; websockets builtin forces TLS `CMakeLists.txt:145-148`; fuzzing forces shared-off `config.mk:222-228`). **No generic dependency layer** — a new trace-propagation flag needing an OTel transport must add its own guard.
- Compile flags only *enable*; runtime behaviour gated by `mosquitto.conf` (established pattern, e.g. `protocol http_api` requires the flag, `conf.c:2456`).

## Proposed new flag names (style-consistent)

`WITH_PROMETHEUS`, `WITH_OTEL_GRPC`, `WITH_OTEL_HTTP`, `WITH_OTEL_TRACING`, `WITH_JSON_LOGGING`.
