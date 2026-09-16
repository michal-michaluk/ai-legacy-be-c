# Review report — Consolidated summary

- Scope: **all uncommitted changes** — `mosquitto/` submodule (OTLP metrics exporter + trace-context tests) and outer workspace (`docs/specs`, `demo/` harness, `.agents/skills/run-k3s`).
- Branch: outer `feat/kubernetes-native`, mosquitto submodule `master` (content modified).
- Method: `review` skill umbrella; 12 aspects across 13 per-aspect reports, produced by independent read-only reviewers and verified (line citations corrected where stale).
- Excluded (generated/ignored): `mosquitto/build-tests/`, `demo/k3s/services/settings-service/target/`, `.playwright-cli/`, `demo/*.mkv|webm`, `.pi/`.

## Verdict

**GO for the implemented scope (C2 OTLP/HTTP metrics, C3 trace carry, C4 opt-in flag); conditional for CI.** The feature is contained behind `WITH_OTEL` (default OFF), the public C API/ABI is untouched, memory management is clean, and the gates pass when actually run: unit `9/9` (404 asserts), trace E2E `11/11`, `24-otel-otlp-metrics.py` exit 0 vs a real `otelcol-contrib`, `zero-overhead.sh` ALL PASS. The blockers are operational/CI, not functional.

## Report index

| Report | Aspect | Verdict |
|---|---|---|
| `report-review-digest.md` | PR/branch digest | Large, mixed concerns |
| `report-review-complexity.md` | Complexity | Well-factored; `config__read_file_core` hotspot |
| `report-review-pragmatic.md` | Pragmatic | Appropriate for a production broker |
| `report-review-architecture.md` | Architecture | Sound; one coupling gap |
| `report-review-backward-compatibility.md` | Backward compat | Intact |
| `report-review-security.md` | Security | SAST/SCA clean; 1 Medium hardening |
| `report-review-bugs.md` | Bugs | 1 High + 4 Medium edge cases |
| `report-review-performance.md` | Performance | Off hot path; 2 avoidable atomics |
| `report-review-tests.md` | Tests | Real assertions; 2 gaps |
| `report-review-reality-check.md` | Reality check | GO (built + ran) |
| `report-review-production-readiness.md` | Production readiness | GO WITH MITIGATIONS |
| `report-review-e2e-demo.md` | Demo as E2E proof tool | Genuine, but partial + illustrative |
| `report-review-spec-coverage.md` | Spec coverage vs gates | Mostly compliant |

## Verified working (independently built and run)

- `test/unit/otel/otel-metrics-test` → **9/9 tests, 404/404 asserts PASS**.
- `ctest -R trace-context` → **11/11 PASS** (serial; parallel needs `--resource-spec-file`).
- `test/broker/24-otel-otlp-metrics.py` vs real collector → **exit 0** (G-C2.1–G-C2.5).
- `test/otel/zero-overhead.sh` → **ALL PASS** (G-C4.1, G-C4.2, G-C4.4, D11).
- `ffprobe demo/kubernetes-native.chapters.mkv` → 7 real embedded chapters.

## Consolidated findings (deduplicated)

| ID | Sev | Finding | Evidence | Reports |
|---|---|---|---|---|
| F1 | High | **G-C2 E2E is not hermetic**: skips (`exit 77`) without the gitignored ~354 MB `otelcol-contrib`, so the C2 gate silently passes-by-skipping on a clean checkout / CI. | `test/broker/24-otel-otlp-metrics.py:42-49` | tests T1, reality R1, spec F2 |
| F2 | High | **Startup uptime can be `0`/negative**: `db.now_s` captured (`src/mosquitto.c:457`) before `start_time` (`src/sys_tree.c:183`); `(uint64_t)uptime` then underflows and the first batch has `startTimeUnixNano > timeUnixNano`. Latent (not reproduced; E2E passed by timing). | `src/sys_tree.c:270`, `src/otel_metrics.c:595-601` | bugs B1 |
| F3 | Medium | **`sys_interval 0` not gated**: exporter thread runs while `sys_tree__update()` is never called, so uptime stays `0` and gauges never refresh. | `src/loop.c:205-207`, `src/sys_tree.c:170-184`, `src/mosquitto.c:566` | arch F1, bugs B2, spec Q3 |
| F4 | Medium | **G-C4.3 ("existing suite green") has no committed assertion** anywhere. | `test/otel/README.md:31-33`; no CI gate | spec F2 |
| F5 | Medium | **Failure classification gaps**: 3xx / non-200 2xx → transient; 429/408 → permanent; and the "retrying next interval" log is false (no retry). | `src/otel_metrics.c:343-352,543-545` | bugs B3, perf P7 |
| F6 | Medium | **Unbounded collector response buffering** → broker OOM via hostile collector, body then parsed by cJSON. | `src/otel_metrics.c:395-409,517` | security S1, bugs B6 |
| F7 | Medium | **Shutdown blocks up to `otlp_timeout` (default 10 s)** when the thread is inside `curl_easy_perform`. | `src/otel_metrics.c:488,737-741` | bugs B5, perf P4, prod P1 |
| F8 | Medium | **No operator docs** for the four `otlp_*` keys (no entry under `mosquitto/man/`). | `mosquitto/man/` (no `otlp`); `spec.md:181-184` | prod P3 |
| F9 | Medium | **3 emitted `$SYS` alias topics not exported** (clients/inactive, clients/active, store/messages/count) — deviation from A1 1:1. | `src/sys_tree.c:54,55,57` vs `src/otel_metrics.c:55-118` | spec F1 |
| F10 | Medium | **Demo scenarios 2–4 are illustrative** (printed, unasserted) and cover only a gate subset. | `demo/scripts/metrics-summary.sh`, `demo/scenarios.md` | e2e D1, D2 |
| F11 | Medium | **`curl_slist_append` return unchecked** (3 sites) → silent header loss + leak on OOM. | `src/otel_metrics.c:466,467,475` | bugs B4 |
| F12 | Low | Config keys appended to the ~1972-line `config__read_file_core` (lizard can't parse it). | `src/conf.c:1003-2974`, `:2345-2372` | complexity C1 |
| F13 | Low | No libcurl protocol restriction; `otlp_endpoint` scheme unvalidated. | `src/otel_metrics.c:484`, `src/conf.c:2347` | security S2, prod P4 |
| F14 | Low | `otlp_headers` not validated for CR/LF/control chars; sanitiser doesn't redact query/fragment. | `src/otel_metrics.c:432-446,652-673` | security S3, S4 |
| F15 | Low | OOM from `config_add_header` misreported as `MOSQ_ERR_INVAL` "Invalid value"; `config_defaults` ignores strdup failure. | `src/conf.c:2360-2365`, `src/otel_metrics.c:277-283` | bugs B7, B8 |
| F16 | Low | Dead code `otel_metrics__init()` (never called). | `src/otel_metrics.c:818`, `src/otel_metrics.h:135` | arch F3, pragmatic P1, bugs B9 |
| F17 | Low | Make build compiles `otel_metrics.o` unconditionally (empty TU when OFF); `make broker` full link untested on the make leg. | `src/Makefile:76` | arch F4, spec F4 |
| F18 | Info | Avoidable hot-path atomics: seq_cst uptime store per loop iteration + locked RMW per counter (only when `WITH_OTEL=ON`). | `src/sys_tree.c:270,201-214` | perf P1, P2 |
| F19 | Info | Demo carries a hardcoded dev Basic credential; k3s manifests miss `securityContext` (deployment scope). | `demo/k3s/k8s/10-otel-collector.yaml:30` | security S6, S7 |

## Cross-report agreement (high confidence)

- **Zero-overhead/flag containment is correct and proven** — default OFF, OFF build adds no symbol/dependency/config surface (arch, backward-compat, prod, spec, reality).
- **Public C API/ABI untouched**; `struct metric` is broker-private (backward-compat).
- **Memory management clean** on normal and error paths — no leak/double-free/UAF (bugs).
- **Off the broker event loop** — all curl I/O on the dedicated thread (perf, arch).
- **`sys_interval 0`**, **shutdown timeout**, and **C2-hermeticity** each independently found by 2–3 reports.

## Corrections applied during verification

- `report-review-reality-check.md` R3 direction fixed: uptime stays `0`, not epoch-sized.
- `report-review-architecture.md` / `report-review-backward-compatibility.md`: stale `file:line` citations corrected.
- `report-review-digest.md`: `24-*.py:39` → `:38`.
- `report-review-production-readiness.md`: `config.mk:153` → `:155`.

## Recommendation

1. Fix **F1** (provision the collector, or fail rather than skip) before relying on CI.
2. Fix **F2** (capture broker start instant once; clamp uptime `>= 0`) — cheap, removes a flaky gate.
3. Address **F3, F5, F6, F7** before any non-demo deployment.
4. Add **F8** man-page docs and the **F4** regression gate.
5. **F16** (delete dead `otel_metrics__init`) is a one-line cleanup.
