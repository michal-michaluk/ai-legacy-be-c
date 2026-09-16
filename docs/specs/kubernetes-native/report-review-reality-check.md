# Review report — Reality check

- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: `.agents/skills/review/references/reality-check-instructions.md` (independent verification, read-only; separate verified vs not-run)
- Evidence base: built/ran the feature; commands:
  - `cmake -S mosquitto -B .agents/tmp/build-otel-on-tests -G Ninja -DWITH_TESTS=ON -DWITH_BUNDLED_DEPS=ON -DWITH_OTEL=ON` + `cmake --build ... --target otel-metrics-test` → **linked OK**; broker binary `src/mosquitto` **built**
  - `.agents/tmp/build-otel-on-tests/test/unit/otel/otel-metrics-test` → **9/9 tests, 404/404 asserts PASS**
  - `ctest -R 'trace-context'` in `mosquitto/build-tests` (WITH_WEBSOCKETS=ON) → **11/11 PASS**
  - `BUILD_ROOT=... python3 mosquitto/test/broker/24-otel-otlp-metrics.py` against real `otelcol-contrib` → **exit 0** (G-C2.1–G-C2.5)
  - `bash mosquitto/test/otel/zero-overhead.sh .agents/tmp/build-otel-off .agents/tmp/build-otel-on` → **ALL PASS: G-C4.1, G-C4.2, G-C4.4, D11**
  - `ffprobe demo/kubernetes-native.chapters.mkv` → 7 real chapters embedded

## Verdict
✅ **Ready for the implemented scope (C2 OTLP/HTTP metrics, C3 trace carry, C4 opt-in flag)** — every locally-runnable gate I exercised passes end-to-end against a real broker and a real OpenTelemetry Collector; the remaining unverified items are environment-heavy (k3s, demo re-record, full regression suite), not functional gaps.

## Verified (what actually works)
- **C4 / G-C4.1, G-C4.2, G-C4.4, D11**: `zero-overhead.sh` ALL PASS — OFF binary has no `otel`/`curl` symbols and no libcurl dep; ON adds only libcurl; `report_features()` differs only by the OpenTelemetry line; OFF rejects all four `otlp_*` keys and the `$SYS` topic inventory is identical OFF vs ON.
- **C2 / G-C2.1–G-C2.5**: `test/broker/24-otel-otlp-metrics.py` exits 0 against a real `otelcol-contrib` — delivery (Sum/monotonic/CUMULATIVE/unit/resource attrs), golden counter payload, non-blocking under a hung exporter, transient recovery + 5xx, and permanent 4xx.
- **C3 / G-C3.1–G-C3.5**: 11/11 trace-context E2E pass serially (byte-identical carry over QoS0/1/2, retained, persistent-across-restart, will, shared, WebSocket, bridge; 3.1.1 negative; oversize whole-message drop; regression).
- **Unit classification table**: pinned 1:1 to the emitted `$SYS` set; 404 asserts pass.
- Build cleanliness: broker compiles `WITH_OTEL=ON` and links system `libcurl` + `Threads` (`mosquitto/src/CMakeLists.txt:174-182`).

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| R1 | Medium | The G-C2 proof harness depends on an external gitignored collector binary and **skips** (exit 77) when absent, so "production-ready" CI on a clean checkout would not actually run the C2 gate. | `mosquitto/test/broker/24-otel-otlp-metrics.py:42-49`; `.agents/tmp/otelcol/` gitignored | Provision the collector in CI, or fail instead of skip when `WITH_OTEL` is enabled. |
| R2 | Low | `cmake --build` of the full tree fails on man-page targets (xsltproc/docbook), not on code. The default build of this workspace cannot complete the doc targets; anyone building naively sees a red build. | `.agents/tmp/review/build-on.log:559-574` (`compilation error: ... manpage.xsl`); broker + unit target build fine | Pre-existing/environmental; document `-DWITH_DOCS=OFF` for the dev loop (the demo Dockerfile already sets it). |
| R3 | Low | With `sys_interval 0` + `WITH_OTEL`, `sys_tree__update()` is never called (`src/loop.c:205-207`), so `uptime_current` stays at its initial `0`: the export thread still runs but `sys_tree__uptime()` reports uptime 0. `start_time` is likewise only set when `sys_interval != 0`. Direction is stale/zero, not epoch-sized. Not exercised (default `sys_interval=10`). | `mosquitto/src/sys_tree.c:270` (write site), `src/sys_tree.c:170-184` (init guard / `start_time`), `src/loop.c:205-207` (guarded caller) | Skip/disable `otel_metrics__start` when `sys_interval == 0`, or refresh uptime independently of the `$SYS` cadence; add an edge test. |
| R4 | Low | `config__copy` transfers OTLP config by shallow move + `otel_metrics__reload`, which only logs a deferred-change warning; reload ownership/lifecycle (double-free/leak across repeated reloads) is not test-covered. | `mosquitto/src/conf.c:743-749`; `mosquitto/src/otel_metrics.c:521-561` | Add a reload unit test that copies configs twice and asserts ownership; confirm no leak. |
| R5 | Low | k3s deployment (spec D12) is claimed working but is only supported by a hand-written `validation-report.md`; I could not re-run it here (no podman/k3s). | `docs/specs/kubernetes-native/validation-report.md:18-42`; harness `.agents/skills/run-k3s/scripts/run_k3s.py:491-548` | Mark k3s proof as environment-dependent; re-run `run-k3s verify` in a controlled environment before relying on it. |

## Not run / unverified (explicit)
- `run-k3s.py verify` / `deploy` (needs podman + k3s; not started).
- Demo video **re-recording** (needs `ttyd`, `node`, `ffmpeg`, `playwright-cli`); the committed `.webm`/`.chapters.mkv` are real but are existing artifacts.
- G-C4.3 "existing CTest suite green" — not executed (only trace subset + otel unit target). `test/otel/README.md:26-31` states the suite has pre-existing environmental failures.
- Legacy `make broker` full link (only the `otel_metrics.o` compile is exercised by `zero-overhead.sh`).
- Hover/LSP-level ABI check: no public header (`include/mosquitto/**`) changes observed in `git diff --stat`.

## Functional completeness
- C2, C3, C4: ~100% of the specified gates **verified locally** against real components.
- C1 (Prometheus) and C5 (JSON logging): explicitly deferred by the spec, out of this change — not a gap.

## Deployment decision
**GO** for the implemented C2/C3/C4 scope; **conditional** for CI: fix R1 (make the collector a first-class test dependency) so the C2 gate cannot silently skip.
