# Review report — Tests

- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: `.agents/skills/review/references/review-tests/instructions.md`; repo guides `docs/test/unit-tests.md`, `docs/test/broker-e2e-tests.md`, `docs/test/testing-and-build.md`
- Evidence base: `mosquitto/test/unit/otel/otel_metrics_test.c`, `mosquitto/test/broker/{24-otel-otlp-metrics.py,25-trace-context-*.py,trace_context_helper.py}`, `mosquitto/test/otel/{collector.yaml,zero-overhead.sh}`, registration diffs; commands run:
  - `cmake --build .agents/tmp/build-otel-on-tests --target otel-metrics-test` + run → 9 tests / 404 asserts, PASS
  - `ctest -R 'trace-context'` (serial) in `mosquitto/build-tests` → **11/11 PASS**
  - `BUILD_ROOT=.agents/tmp/build-otel-on-tests python3 mosquitto/test/broker/24-otel-otlp-metrics.py` → **exit 0** (G-C2.1–G-C2.5 exercised)
  - `ctest -R 'trace-context' -j4` (no `--resource-spec-file`) → 6 fail (port collision, harness artifact)

## Verdict
New behaviour is covered by real, non-tautological assertions at both unit and E2E level and the gates pass when run; the main gaps are (a) the G-C2 E2E silently *skips* on a fresh checkout because it needs a gitignored external `otelcol-contrib` binary, and (b) the `sys_tree__*` current-value accessors and exporter lifecycle/reload have no direct test.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| T1 | High | G-C2 E2E is **not hermetic**: it requires a ~354 MB `otelcol-contrib` not in the repo and `sys.exit(77)` (CTest SKIP) when absent → the whole C2 gate silently skips instead of failing. Contradicts `docs/test/broker-e2e-tests.md` "Do not silently skip because a fixture, executable, or configuration is missing." | `mosquitto/test/broker/24-otel-otlp-metrics.py:42-49`; binary only at gitignored `.agents/tmp/otelcol/otelcol-contrib` | Pin/provision the collector (CI download step or vendored path) and make a missing declared capability fail, or document the skip as an explicit external prerequisite. |
| T2 | Medium | Current-value metric source is untested at unit level: no test calls `sys_tree__metric_value`, `sys_tree__load_value`, `sys_tree__uptime`. The `is_max` max-computation and "non-emitted → `MOSQ_ERR_INVAL`" rules are only exercised implicitly via E2E-24. | `grep -rn 'sys_tree__metric_value|sys_tree__load_value|sys_tree__uptime' mosquitto/test/` → no matches; impl `mosquitto/src/sys_tree.c:217-260` | Add table-driven unit cases (emitted vs non-emitted enum, is_max vs running value) per `docs/test/unit-tests.md`. |
| T3 | Medium | Exporter lifecycle/transport logic has no unit coverage: `otel_metrics__start/stop/reload`, thread main, URL/header building, `otel__extract_rejected` are only reached through the E2E. Mutation of these paths would not be caught fast. | `grep` in `mosquitto/test/` for `otel_metrics__start|stop|reload` → no matches; impl `mosquitto/src/otel_metrics.c:392-560` | Add unit tests for `otel_metrics__reload` equality/deferred-change and header/URL construction (pure functions, no I/O). |
| T4 | Low | `-j` parallel execution of the trace suite fails without a resource spec due to ephemeral-port reuse; the project relies on `--resource-spec-file`. Plain `ctest -jN -R trace-context` is flaky. | `.agents/tmp/review/ctest-trace.log` (`Address already in use`, 6/11 fail at `-j4`); `mosquitto/CMakeLists.txt:353-357` `RESOURCE_GROUPS`; pass at serial | Document/ensure resource spec is always used (project `CMAKE_CTEST_ARGUMENTS` already does); consider binding listener port at accept time. |
| T5 | Low | Unit test includes production `sys_tree.c` and compiles `otel_metrics.c` directly to read the *real* emitted topic array; this is deliberate drift-prevention, but it couples the unit test to internal aggregate initializers (`_Atomic` layout) and no longer tests the accessor API through its header. | `mosquitto/test/unit/otel/otel_metrics_test.c:22` (`#include "sys_tree.c"`), `.../CMakeLists.txt:3` | Acceptable; add a thin accessor test compiled against the real `sys_tree.c` so the coupling buys coverage, not just name pinning. |
| T6 | Low | `TEST_aliases_are_known` hard-asserts `aliases == 3`; a deliberate tripwire, but brittle to a legitimate `$SYS` alias addition and could be mistaken for a bug. | `mosquitto/test/unit/otel/otel_metrics_test.c:200-211` | Keep, but assert against an explicit expected alias list so the failure says *which* alias changed. |
| T7 | Low | `memtrack_enabled()` derives from env `INC_MEMTRACK`; on a build with `WITH_MEMORY_TRACKING` off the two heap metrics are dropped from the expected set, but the unit test compiles with `WITH_MEMORY_TRACKING` unconditionally — the two disagree on which matrix cell is tested. | `mosquitto/test/broker/24-otel-otlp-metrics.py:107-109`; `mosquitto/test/unit/otel/CMakeLists.txt:8-12` | Build the unit target with/without the flag, or assert both matrices, to match the E2E. |

## Good practices / strengths
- Assertions are real, not tautological: unit test drives the deterministic enum→(name,type,unit) table against the *actual* emitted `$SYS` topics (`otel_metrics_test.c:113-155`) and parses the produced OTLP/JSON with cJSON (`:231-330`).
- E2E-24 asserts exact golden counter values driven by the test (5 publishes) plus kinds/units/monotonic cumulative temporality and resource attrs (`24-otel-otlp-metrics.py:255-330`).
- Trace suite proves *byte-identical* carry: `receive_publish` compares the full received packet and `assert_user_properties` re-parses the received packet (`trace_context_helper.py:44-67`); regression case covers duplicate keys/order/vendor keys (`25-trace-context-regression.py:26-52`).
- Gating is idiomatic: `require_features` for OTEL/persistence/websockets/bridge; CMake sets feature env via `option_env` (`mosquitto/CMakeLists.txt:36-40,345-357`).
- Coverage of the full delivery-path matrix (QoS0/1/2, retained, persistent-across-restart, will, shared, WebSocket both directions, bridge both directions, 3.1.1 negative, oversize, regression) matches G-C3.2–G-C3.5 exactly.

## Open questions / unverified
- G-C4.3 ("existing CTest suite green") not run here — `test/otel/README.md` states the suite has pre-existing environmental failures; unknown whether those are pre-existing or new.
- Full-suite `ctest` (with resource spec) not executed; only `-R trace-context` and the otel unit target.
