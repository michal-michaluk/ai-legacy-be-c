# Review report — E2E demo harness

- Scope: uncommitted changes (workspace) — `demo/` evaluated **only** as an E2E verification / proof tool for spec gates; the Rust settings-service application code is out of scope.
- Method: read the demo harness, mapped every scenario and command to the spec §6 gates; cross-checked against the automated CTest gates; inspected the recording artifact.
- Evidence base: `demo/README.md`, `demo/scenarios.md`, `demo/record.js`, `demo/chapters.json`, `demo/scripts/*.sh|*.mjs`, `mosquitto/test/otel/zero-overhead.sh`, `mosquitto/test/otel/collector.yaml`, `.agents/skills/run-k3s/SKILL.md`, `.agents/skills/run-k3s/scripts/run_k3s.py`, `demo/k3s/**`; `ffprobe demo/kubernetes-native.chapters.mkv`; the three runnable gates re-executed (see `report-review-reality-check.md`).

## Verdict
The `demo/` harness is a **genuine** end-to-end proof tool for a *subset* of the gates — it drives the real broker, a real `otelcol-contrib`, real MQTT CLI clients and the real `zero-overhead.sh`, and its video is a real recording; but scenarios 2–4 are **illustrative (printed output, no assertion)** and only G-C2.1/2.2, G-C3.1/3.3, G-C4.1/4.2/4.4 are demonstrated, the rest being proven only by CTest.

## Demonstrated component reality
| Element | Real? | Evidence |
|---|---|---|
| Broker | Yes — builds from this repo with `WITH_OTEL=ON`; OFF variant also used | `demo/scripts/start-broker.sh` uses `.agents/tmp/build-otel-$mode/src/mosquitto`; `demo/k3s/docker/mosquitto-otel.Dockerfile:24-31` builds `-DWITH_OTEL=ON` from source |
| Collector | Yes — real `otelcol-contrib`, OTLP/HTTP receiver → JSONL | `demo/scripts/start-collector.sh`; `mosquitto/test/otel/collector.yaml` |
| MQTT clients | Yes — real `mosquitto_sub`/`mosquitto_pub` with `-D publish user-property` | `demo/record.js:59-78` |
| Recording | Yes — real chaptered video, 7 embedded chapters | `ffprobe` lists chapters matching `demo/chapters.json`; `demo/*.webm|.mkv` present |
| Zero-overhead | Yes — the asserting script itself | `demo/record.js:83` runs `mosquitto/test/otel/zero-overhead.sh` (verified ALL PASS) |

## Gate coverage vs spec §6
| Gate | Demonstrated? | How real is the proof |
|---|---|---|
| G-C4.4 capability report | Scenario 1 | Real: broker startup lines grepped by `start-broker.sh`; ON export line shown |
| G-C2.1 delivery / G-C2.2 payload | Scenario 2 | **Illustrative**: `metrics-summary.sh` *prints* resource attrs, kinds, temporality, headline counters and one raw OTLP/JSON object — no assertion; a human must eyeball |
| G-C3.1 happy carry | Scenario 3 | **Illustrative**: subscriber `-F '%P | %p'` printed and `cat`-ed — no assertion |
| G-C3.3 3.1.1 negative | Scenario 4 | **Illustrative**: printed `[%P]` output — no assertion |
| G-C4.1/G-C4.2/G-C4.4 zero-overhead | Scenario 5 | **Real assertion**: runs `zero-overhead.sh`, which exits non-zero on any failed gate |
| G-C2.3 non-blocking | No | proven only by `test/broker/24-otel-otlp-metrics.py` |
| G-C2.4 transient / G-C2.5 permanent | No | proven only by `test/broker/24-otel-otlp-metrics.py` |
| G-C3.2 delivery paths (QoS1/2, retained, persistent, will, shared, WS, bridge) | No | proven only by `test/broker/25-trace-context-*.py` |
| G-C3.4 oversize / G-C3.5 regression | No | proven only by the CTest trace suite |
| G-C4.3 existing suite green | No | not run anywhere (see reality-check) |

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| D1 | Medium | The demo's C2/C3 scenarios (2,3,4) are **staged/illustrative**: they print correct-looking output but contain no machine assertion, so the video alone cannot prove pass/fail; the authoritative proof lives in CTest. | `demo/scripts/metrics-summary.sh` (prints, `exit 0` even when file empty); `demo/record.js:59-78` (`cat` of subscriber output) | State explicitly in `demo/scenarios.md` that C2/C3 assertions are in `test/broker/24-*` and `25-*`; optionally make the demo scripts `exit 1` on a missing expected metric/user-property. |
| D2 | Medium | Gate coverage is a strict subset: G-C2.3–C2.5, G-C3.2, G-C3.4, G-C3.5 are absent from the 5 scenarios. | `demo/scenarios.md` (5 scenarios); spec §6 gate list | Add chapters or explicitly cross-reference the CTest cases for the uncovered gates. |
| D3 | Low | Reproducibility depends on gitignored artifacts (`.agents/tmp/build-otel-{off,on}`, `.agents/tmp/otelcol/otelcol-contrib`) plus `ttyd`/`node`/`ffmpeg`/`playwright-cli`; a clean checkout cannot reproduce the demo from `demo/README.md` alone. | `demo/README.md` Prerequisites; `demo/record.js:41-44` hardcodes `.agents/tmp/build-otel-on` | Add a `demo/scripts/bootstrap.sh` that builds the two trees and fetches/provisions the collector, or pin versions/URLs in README. |
| D4 | Low | The stronger k3s proof (`run-k3s verify`) asserts C2 metrics and a *single* trace id carrying both `mqtt.publish` and `mqtt.receive` across the two services, but it parses the collector **debug-exporter text log** rather than structured JSONL, and was not re-run here (no podman/k3s). | `.agents/skills/run-k3s/scripts/run_k3s.py:486-548` (`TRACE_ID_RE`/`SPAN_NAME_RE` over `collector` log text); `docs/specs/kubernetes-native/validation-report.md:18-42` | Prefer asserting on structured exporter output; mark k3s verification as environment-dependent and re-run before relying on it. |
| D5 | Low | Scenario 2 uses a hardcoded demo OTLP port `43183` and an `otlp_export_interval 1`; the OFF broker is started without `otlp_*` keys specifically so it "starts cleanly" — correct, but the capability contrast (scenario 1) is a log-line comparison, not a behavioural one. | `demo/scripts/start-broker.sh:9-20`; `demo/record.js:52-55` | Fine as a capability demo; the behavioural contrast is already asserted by `zero-overhead.sh` (D11). |

## Good practices / strengths
- The harness runs **real** components end-to-end: a broker built from this repo, the real `otel-collector-contrib`, real MQTT 5 clients with User Property injection, and the authoritative `zero-overhead.sh` — no mocks.
- Scenario 5 is a real gate: it invokes the same asserting script I independently re-ran (ALL PASS), so `demo` and CTest agree on the C4 proof.
- The recording is verifiable: `ffprobe` confirms 7 chapters embedded with the exact titles/timestamps in `demo/chapters.json` (`demo/record.js:45-48`, `mark()`).
- `demo/scripts/serve-demo.mjs` implements HTTP Range so reviewers can seek; `demo/README.md` gives the public-sharing path.
- Cleanup is scripted (`demo/scripts/stop-all.sh`, `demo-setup.sh` reset) and the driver starts/stops broker+collector itself.

## Open questions / unverified
- Not re-recorded here: does `record.js` still reproduce cleanly on the current build (needs a GUI-capable `playwright-cli` + `ttyd`)? Unverified.
- k3s `verify` unverified in this environment.
- Whether the settings-service `traceparent` log line (validation-report §3) is a real extraction vs a log of the inbound property — not assessed (application code out of scope).
