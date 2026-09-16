# Review report — Production Readiness

- Scope: uncommitted changes (mosquitto submodule new OTLP/HTTP exporter + workspace demo manifests)
- Method: `/Users/michal/.agents/skills/review/references/review-production-readiness-instructions.md` (GO/NO-GO lens)
- Evidence base: full read of `mosquitto/src/otel_metrics.c|.h`; `git diff` of `src/conf.c`, `src/mosquitto.c`, `src/sys_tree.c|.h`, `src/CMakeLists.txt`, `make/broker.mk`, `config.mk`; `mosquitto/test/otel/zero-overhead.sh`; `mosquitto/test/broker/24-otel-otlp-metrics.py`; `mosquitto/man/` (no `otlp` entries); `docs/specs/kubernetes-native/spec.md`

## Verdict
GO WITH MITIGATIONS. The feature is default-OFF in both build systems and has correct thread/curl lifecycle and explicit failure classification. It is not production-hardened: exporter shutdown can block SIGTERM for up to `otlp_timeout`, config changes need a restart with only a log notice, and there is no operator documentation for the new keys.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| P1 | Concern (Resilience/Deployment) | `otel_metrics__stop()` joins the export thread; if that thread is mid-`curl_easy_perform` the join waits up to `otlp_timeout` (default 10 s), delaying broker graceful shutdown/SIGTERM. | `mosquitto/src/otel_metrics.c:732-752` (join), `:488` (CURLOPT_TIMEOUT), `mosquitto/src/mosquitto.c:346` (`stop` first in `post_shutdown_cleanup`) | Bound shutdown: use a short `CURLOPT_CONNECTTIMEOUT`, keep an abort handle / `CURLOPT_XFERINFOFUNCTION` to cancel in-flight, or cap `otlp_timeout`. Document max shutdown delay. |
| P2 | Concern (Operational semantics) | Reload does not reconfigure the running exporter; a changed OTLP config is only logged ("not reconfigured until restart"). Correct by design (thread owns a snapshot) but surprising operationally. | `mosquitto/src/otel_metrics.c:781-786`; `mosquitto/src/conf.c:747-750` | Document the restart requirement explicitly (man page + spec), or apply config by restarting the thread on reload. |
| P3 | Concern (Docs) | No operator documentation for `otlp_endpoint` / `otlp_export_interval` / `otlp_headers` / `otlp_timeout`: no `mosquitto.conf.5` man-page entry exists (no `otlp` string anywhere under `mosquitto/man/`). Spec's "operator docs" property is unmet. | `mosquitto/man/` (grep `otlp` → none); `docs/specs/kubernetes-native/spec.md:80,107,181-184` | Add the four keys + defaults to `mosquitto/man/mosquitto.conf.5.xml`. |
| P4 | Concern (Validation) | `otlp_endpoint` is not syntax/scheme validated; a bad value only surfaces as periodic export warnings, not a config error. | `mosquitto/src/conf.c:2345-2349`; `mosquitto/src/otel_metrics.c:449-467` | Validate scheme (`http`/`https`) and basic URL form at parse time (also see Security S2). |
| P5 | Recommendation (Observability) | The exporter reports only failures (`WARNING` transient/partial, `ERR` permanent). No success indication and no exporter-level counters, so an operator cannot confirm exports are landing. | `mosquitto/src/otel_metrics.c:532-550` | Add an info/debug success log or an exporter failure counter; align with collector-side signals. |
| P6 | Info | First export runs immediately at thread start (before the first interval wait), so enabling the flag issues an HTTP POST at broker startup; with the default endpoint `http://localhost:4318` and no collector this logs a transient warning every interval. | `mosquitto/src/otel_metrics.c:625-632`, defaults `:277-281` | Intended push behaviour; document so operators don't treat startup warnings as errors. |
| P7 | Info | Exporter reads metrics across independent atomic loads (values not a single consistent snapshot); acknowledged in the API doc. | `mosquitto/src/sys_tree.h:126-142`, `mosquitto/src/otel_metrics.c:559-600` | Acceptable for scraping; no action. |

## Good practices / strengths
- Default-OFF safety: `WITH_OTEL=no` (`mosquitto/config.mk:155`) and `option_env(WITH_OTEL ... OFF)` (`mosquitto/src/CMakeLists.txt:97`); OFF build adds no symbol, no libcurl dependency, no config surface (proved by `mosquitto/test/otel/zero-overhead.sh`).
- Fail-fast dependency rule: `WITH_OTEL` without `WITH_SYS_TREE` aborts configure/make clearly (`mosquitto/src/CMakeLists.txt:182-186`, `mosquitto/make/broker.mk:37-39`).
- Clean resource lifecycle: `curl_global_init` before `pthread_create`, `curl_global_cleanup` on both create-failure and stop; `pthread_join`, mutex/cond destroy, config + string cleanup; `stop()` is idempotent (`mosquitto/src/otel_metrics.c:680-752`).
- Failure classification matches spec §5.2: 200→ok/partial, 4xx→permanent (drop, no retry storm), 5xx/connect→transient (retry next interval); every outcome logged (`mosquitto/src/otel_metrics.c:343-354,532-550`).
- No unbounded in-flight accumulation: one request per interval, no retry queue, so a down/hung collector does not grow broker memory.
- Config validation: `otlp_export_interval`/`otlp_timeout` reject `< 1` (`mosquitto/src/conf.c:2354,2369`); E2E covers delivery, non-blocking against a hung collector, transient and permanent failure (`mosquitto/test/broker/24-otel-otlp-metrics.py` G-C2.1–G-C2.5).
- Capability is reported in startup banner (`mosquitto/src/mosquitto.c:331-333`).

## Open questions / unverified
- Exact worst-case shutdown delay not measured end-to-end (bounded analytically by `otlp_timeout`; default 10 s). Marked unverified.
- No chaos/soak test observed for repeated collector restarts or long collector hangs; only the E2E hang scenario (`G-C2.3`) exists.
- Whether reuse of an existing collector-side `/v1/metrics` auth (bearer via `otlp_headers`) is required in the target cluster is unverified; the demo collector path uses `debug`/`file` exporters (no auth).
