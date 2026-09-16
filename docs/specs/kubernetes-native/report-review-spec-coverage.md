# Review report — Spec coverage
- Scope: uncommitted changes (mosquitto submodule + workspace), branch `feat/kubernetes-native`
- Method: references/review-spec-coverage-instructions.md (spec-coverage audit)
- Evidence base (all read in full):
  - spec: `docs/specs/kubernetes-native/spec.md`, `trace-context-convention.md`, `validation-report.md`
  - code: `mosquitto/src/otel_metrics.c`/`.h`, `sys_tree.c`/`.h`, `conf.c`, `mosquitto.c`, `mosquitto_broker_internal.h`, `http_api.c`
  - build: `mosquitto/config.mk`, `make/broker.mk`, `src/CMakeLists.txt`, `src/Makefile`
  - tests: `mosquitto/test/broker/24-otel-otlp-metrics.py`, `25-trace-context-*.py` (11) + `trace_context_helper.py`, `test/unit/otel/**`, `test/otel/{collector.yaml,README.md,zero-overhead.sh}`, registrations (`test/broker/{CMakeLists.txt,test.py,Makefile}`, `test/unit/CMakeLists.txt`)
  - harness: `test/mosq_test.py` (`require_features`→77, `do_ping`, `expect_packet`), `CMakeLists.txt:346-363` (`SKIP_RETURN_CODE 77`)

## Verdict
**Mostly compliant.** C2 (G-C2.1–G-C2.6) and C3 (G-C3.1–G-C3.5) are genuinely covered by real assertions. C4 is covered except **G-C4.3 (no-regression) has no committed assertion**, and D11's "byte-identical `$SYS` output" is only asserted at topic-inventory level. One metric-set deviation: 3 emitted `$SYS` alias topics are not exported.

## Gate matrix
| Gate | Status | Implementation (file:line) | Asserting test/scenario | Notes |
|---|---|---|---|---|
| G-C2.1 delivery | Covered | `mosquitto/src/otel_metrics.c:188` (`encode_json`), `:449` (`otel__post`, `/v1/metrics` + JSON headers), `:559` (`otel__export_once`); `mosquitto/test/otel/collector.yaml` | `24-otel-otlp-metrics.py:293` `assert_delivery` (name, `sum`, `aggregationTemporality==2`, `isMonotonic`, `unit`, `startTimeUnixNano<=timeUnixNano`, resource attrs `service.name/version/instance.id`, scope `mosquitto`) | `require_features(["WITH_OTEL"])` at `:38` |
| G-C2.2 payload | Covered (1 deviation, F1) | `otel_metrics.c:55` `metric_meta[]`, `:89` `load_meta[]`, `:138` `uptime_meta` | `24-...:312` `assert_payload` (exact name set, per-name unit, golden counters `publish_messages_received/sent`, `publish_bytes_received/sent`=`5×len`, monotonic non-decreasing) + `test/unit/otel/otel_metrics_test.c:111/143/224` | F1: 3 emitted `$SYS` aliases intentionally not exported (`src/sys_tree.c:54,55,57`) |
| G-C2.3 non-blocking | Covered | `otel_metrics.c:625` `otel__thread_main` (dedicated thread), `:711` `pthread_create` | `24-...:378` `test_non_blocking` (`HangingServer` proves export blocked; `blocked <= max(baseline*3, baseline+0.5)` and `< 2.0`) | Baseline = same broker with collector port closed |
| G-C2.4 transient | Covered | `otel_metrics.c:343` `classify` (5xx/timeout→transient), `:532` `otel__report` | `24-...:413` `test_transient_recovery` (connect error→warning, recovers), `:439` `test_transient_5xx` (HTTP 503 warning + keeps exporting) | |
| G-C2.5 permanent | Covered | `otel_metrics.c:343` `classify` (4xx→permanent), `:532` `otel__report` | `24-...:460` `test_permanent_failure` ("failed permanently", "HTTP 400", `2 <= requests <= 8` over 3 s) | |
| G-C2.6 zero-overhead (folded) | Covered | flag guard (`config.mk:155`, `src/CMakeLists.txt:183-191`) | `test/otel/zero-overhead.sh:183,187` (OFF binary: no otel/curl symbol, no libcurl dep) | Folded into G-C4.2 per `spec.md:348` |
| G-C3.1 happy | Covered | no broker code (F7/D4); `docs/specs/kubernetes-native/trace-context-convention.md` | `25-trace-context-happy.py` (exact packet equality + `assert_user_properties`) | |
| G-C3.2 paths | Covered | generic User Property forwarding (F7) | `25-trace-context-qos.py` (QoS 0/1/2), `-retained.py`, `-persistent.py` (restart + persistence), `-will.py`, `-shared.py` (2 groups), `-websocket.py` (both directions), `-bridge.py` (mqttv50, both directions) | All byte-exact + parsed User Properties |
| G-C3.3 negative | Covered | `lib/send_publish.c` MQTT5 gate (F7) | `25-trace-context-v311-negative.py` (MQTT 3.1.1 subscriber, `expected` has no property block) | |
| G-C3.4 oversize | Covered | `MOSQ_ERR_OVERSIZE_PACKET` whole-message drop (F7) | `25-trace-context-oversize.py` (with-props and no-props oversize both absent via `do_ping`) | `do_ping` fails if a PUBLISH arrives (`test/mosq_test.py:693`) |
| G-C3.5 regression | Covered | no code; byte-carry guarantee | `25-trace-context-regression.py` (5 cases: order, dup keys, no injection, unknown vendor key) | |
| G-C4.1 build matrix | Partial | `config.mk:155`, `make/broker.mk:37-45`, `src/CMakeLists.txt:97,183-191`, `src/Makefile:76` | `zero-overhead.sh:87` (CMake `{none,otel}` full builds), `:99-108` (legacy make compiles `otel_metrics.o`) | F4: legacy **`make broker`** (full link) not asserted — only the guarded TU object |
| G-C4.2 zero-overhead | Covered | flag guards | `zero-overhead.sh:183-204` (OFF no otel/curl syms+deps; ON adds otel/curl; baseline symbol set only grows; sole new dep = libcurl) | Proxy for `spec.md:318` "OFF vs baseline diff empty" |
| G-C4.3 no-regression | **Not covered** | — | none | F2: no committed assertion; `test/otel/README.md:31-33` explicitly excludes it |
| G-C4.4 capability report | Covered | `mosquitto/src/mosquitto.c:330-333` (`report_features`) | `zero-overhead.sh:285` (OFF `OpenTelemetry support NOT available.`; ON `...available.`; only that line differs) | |
| D11 zero-overhead | Partial | `config.mk:155`, `conf.c:2345-2373` | `zero-overhead.sh:293` (OFF rejects all `otlp_*`), `:332` (ON accepts/applies all 4 keys), `:356` (OFF/ON identical `$SYS` **topic inventory**) | F3: "byte-identical `$SYS` output" (`spec.md:112,296`) is not asserted at value level |
| D13 metric typing | Covered | `otel_metrics.c:55-118` (type/monotonic/unit table) | `otel_metrics_test.c:111,143,181` (name derived from real `$SYS` topics; type/monotonic/unit pinned 1:1) | Peaks+samples→Gauge, on-event→monotonic Sum, uptime→Sum `s` |
| §5.2 data model (resource/scope/types/temporality) | Covered | `otel_metrics.c:188-263` | `otel_metrics_test.c:224` (all resource attrs, scope, `asInt` string, `asDouble`, temporality, monotonic); `24-...:293` | |
| §5.2 failure mapping | Partial | `otel_metrics.c:343,532` | `otel_metrics_test.c:356` (classify table); `24-...:413,439,460` | F5: `200+rejected>0` partial-success **warning log** not asserted E2E |
| §5.2 `otlp_headers` | Partial | `conf.c:2358`, `otel_metrics.c:469` (`otel__header_line`) | `otel_metrics_test.c:326` (parse/cleanup); `zero-overhead.sh:332` (accepted/applied) | F6: header not asserted **on the wire** |
| §5.3 convention doc | Covered | `docs/specs/kubernetes-native/trace-context-convention.md` | referenced; scenarios table matches tests | |
| §5.4 flag contract (WITH_OTEL only, default OFF, fail-fast dep, no per-transport flag) | Covered | `config.mk:155`, `make/broker.mk:37-45` (fail-fast if no `WITH_SYS_TREE`), `src/CMakeLists.txt:183-191` | `zero-overhead.sh:138` (default OFF + guard containment); `src/CMakeLists.txt:185` FATAL_ERROR | |

## Spec's own review instructions (spec.md:424 and spec.md:475)
`spec.md:424` (flag wiring) — each requested item verified:
- default OFF — `config.mk:155` `WITH_OTEL=no`; `src/CMakeLists.txt:97` `option_env(... OFF)`; asserted `zero-overhead.sh:133-136`.
- dependency discovered only when enabled — `src/CMakeLists.txt:187-188` `find_package(CURL/Threads REQUIRED)` inside `if(WITH_OTEL)` (line 183); `make/broker.mk:44` `-lcurl` inside the guard.
- valid disabled build — CMake OFF full build (`zero-overhead.sh:87`).
- capability reported — `mosquitto.c:330-333`; asserted `zero-overhead.sh:285`.

`spec.md:475` (otlp-e2e scenario) — each requested item verified:
- ports via `mosq_test.get_port` — `24-...:356,381,415,441,462`.
- unconditional cleanup — `24-...` per-test `try/finally` + `main()` `finally: shutil.rmtree` (`:355-490`).
- exact assertions — set/type/unit/golden values/latency budget/log strings (see matrix).
- feature gating — `require_features(["WITH_OTEL"])` `:38`; CTest `SKIP_RETURN_CODE 77` (`mosquitto/CMakeLists.txt:351`).
- G-C2.1–G-C2.5 — all present; G-C2.6 correctly folded into G-C4.2.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| F1 | Medium | **Metric set is not literally 1:1 with the emitted `$SYS` set (A1/§5.2/D6).** The 3 alias topics `$SYS/broker/clients/inactive`, `.../clients/active`, `.../store/messages/count` are emitted by `$SYS` but have no OTLP meta entry, so OTLP exports 3 fewer topics than `$SYS` publishes. | emitted: `mosquitto/src/sys_tree.c:54,55,57`; omissions: `mosquitto/src/otel_metrics.c:55-118`; acknowledged: `mosquitto/test/unit/otel/otel_metrics_test.c:168` (`TEST_aliases_are_known`) + comment `:56-60` | Either export the aliases or record the exclusion in spec A1/D6 as an explicit, reviewed deviation. |
| F2 | High | **G-C4.3 ("existing CTest suite green") has no committed assertion.** The only C4 harness explicitly states it does not run `ctest`; no CI workflow references the gates. | `mosquitto/test/otel/README.md:31-33`; `test/otel/zero-overhead.sh` (no ctest invocation); no `.github/workflows/*` reference to `WITH_OTEL`/`zero-overhead` | Add a rerunnable check that builds `{none, otel}`, runs the suite and asserts green (or record known-excluded failures). |
| F3 | Medium | **D11 "byte-identical `$SYS` output" is only asserted at topic-inventory level, not values.** | `mosquitto/test/otel/zero-overhead.sh:345-356` (`sys_topics` diff); `test/otel/README.md:29-31` | Assert byte-equality of at least the deterministic `$SYS` values (or amend D11 to explicitly weaken it to topic inventory). |
| F4 | Low | **G-C4.1 make leg is partial:** legacy make only compiles `otel_metrics.o`, never a full `make broker` link with `-lcurl`. | `mosquitto/test/otel/zero-overhead.sh:93-108` | Add a full legacy `make broker` build with `WITH_OTEL=yes`. |
| F5 | Low | **§5.2 partial-success path (`200`, `rejectedDataPoints>0`) warning is not asserted end-to-end.** Only the pure `classify` unit test covers it. | `mosquitto/test/unit/otel/otel_metrics_test.c:356`; no assertion in `mosquitto/test/broker/24-otel-otlp-metrics.py` | Assert the "collector rejected N data points" warning via a stub returning `200` + `partialSuccess`. |
| F6 | Low | **`otlp_headers` values are never asserted on the wire** (only parsed/accepted). | `mosquitto/src/conf.c:2358-2363`; `mosquitto/src/otel_metrics.c:469-491`; `zero-overhead.sh:332` | Add a stub that asserts the configured header arrives. |

## Explicitly deferred (not gaps)
- **C1 / §5.1 Prometheus / `WITH_PROMETHEUS`** — deferred to element §5.1 (`spec.md:545`, `:542`); not implemented here.
- **C5 / §5.5 JSON logging / `WITH_JSON_LOGGING`** — deferred to element §5.5 (`spec.md:545`).
- **OTLP log export (A2)** — deferred; when added it stays under `WITH_OTEL` (`spec.md:546`).
- **§5.6 local test environment + operator docs** — not part of this plan (`spec.md:547`); the collector harness under `mosquitto/test/otel/` is the minimal G-C2 subset.
- **k3s deployment proof (D12)** — explicitly not exercised here (`spec.md:548`).
- **gRPC / protobuf transport** — out of scope (`spec.md:350`, D9; `spec.md:527`).
- **G-C4.1 full matrix `{none, prometheus, otel_http, json_logging, all}`** — spec-sanctioned reduction to `{none, otel}` in this plan (`spec.md:296`).
- **Broker spans, MQTT 3.1.1 trace propagation, K8s artifacts under `mosquitto/`** — out of scope (`spec.md` Scope §).

## Extra deliverables (beyond the plan; not counted as gaps)
- `demo/` (scenarios + chaptered video), `.agents/skills/run-k3s/`, `docs/specs/kubernetes-native/validation-report.md` — supporting §5.6/D10/D12 material; the k3s `verify` gate is recorded in `validation-report.md:44-49`.

## Open questions / unverified
- Q1: Is omitting the 3 `$SYS` aliases from OTLP intentional and accepted, or a spec violation of A1's "nothing more, nothing less"? Spec is silent; only the unit test's comment documents the choice.
- Q2: Should G-C4.3 be automated given the README's claim of "pre-existing environmental failures"? The claim itself is unverified here (suite not run).
- Q3 (unverified): with `WITH_OTEL` and `sys_interval 0`, `$SYS` publishes nothing but the metric source still updates and the exporter would still send — a possible mismatch with A1's "currently emitted" framing. `sys_tree.c:270` refreshes `uptime_current` unconditionally while the header comment (`sys_tree.h`) says "while `$SYS` is enabled".
- Q4 (unverified): E2E correctness of G-C2.3's fixed `2.0 s` / `baseline*3` budget on slow machines was reviewed by inspection only; tests were not executed in this read-only audit.
