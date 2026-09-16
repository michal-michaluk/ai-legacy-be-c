# Review report — Pragmatic
- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: `/Users/michal/.agents/skills/review/references/review-pragmatic-instructions.md`
- Evidence base: read of `src/otel_metrics.c/.h`, `src/conf.c`, `src/sys_tree.*`, `src/mosquitto.c`, `test/**`, `docs/specs/kubernetes-native/spec.md`, `demo/**`, `.agents/skills/run-k3s/**`; `grep` for unused symbols.

## Verdict
**Appropriate complexity** relative to a production C broker: the OTLP exporter is a genuine, spec-justified capability (spec §5.2/D9 chose hand-emit to avoid a protobuf dependency), gated OFF by default. No enterprise-pattern overkill. One small dead-code item and a heavy-but-justified demo harness.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| P1 | Low | `otel_metrics__init()` is declared in the header and defined as an empty function, with **no caller anywhere** (broker, tests, other files). Dead code / context-loss residue. | `mosquitto/src/otel_metrics.h:135`, `mosquitto/src/otel_metrics.c:818-820` | Delete declaration + definition, or wire it into startup if an init step was intended. |
| P2 | Low | Failure handling is spread over three functions for a 4-state outcome: `otel_metrics__classify` (pure), `otel__extract_rejected` (JSON parse), `otel__report` (log switch). Functionally fine; a single call site (`otel__export_once:608`) hides the split. | `mosquitto/src/otel_metrics.c:343-352`, `:508-529`, `:532-547` | Optional: inline `report` into the export path; keep `classify` + `extract_rejected` (both unit-tested). |
| P3 | Low | Export path allocates a fresh `samples` array + JSON string every interval rather than reusing a buffer. At the default 60 s cadence this is negligible; only relevant if interval is set very low. | `mosquitto/src/otel_metrics.c:566-568`, `:603` | Accept as-is; document that very low `otlp_export_interval` increases allocation churn. |
| P4 | Info | Demo harness is large: `demo/` ≈ 6900 LOC (a copied Rust `settings-service` + k3s manifests + recording scripts) plus `.agents/skills/run-k3s/` (SKILL.md 102 + `run_k3s.py` 601). | `demo/`, `.agents/skills/run-k3s/scripts/run_k3s.py:1-601`; `docs/.../validation-report.md:11-20` | Justified by spec F11/Q7 ("local test environment + operator docs"); no simplification required. Flag only that it dominates the diff by line count. |
| P5 | Info | Config key validation covers lower bound only (`< 1` rejected) for `otlp_export_interval`/`otlp_timeout`; no upper bound. | `mosquitto/src/conf.c:2351-2371` | Acceptable; a bound is not required by spec. |

## Complexity assessment vs project scale
- Project scale: **production** broker (eclipse-mosquitto fork), long-lived, public C ABI. Enterprise-grade reliability is warranted here.
- Implementation complexity: **Low–Medium** — one table, one pure encoder, one thread. No abstraction layers, no repository/factory patterns, no added infrastructure beyond libcurl (already a discovered dependency).
- The hand-rolled OTLP/JSON encoder instead of `opentelemetry-cpp`/protobuf is a **deliberate simplification** documented in the spec (`spec.md` F9/D9), not over-engineering.

## Developer experience
- Zero-overhead OFF path is verifiable in one command (`test/otel/zero-overhead.sh`), including symbol/dependency diff and `$SYS` inventory diff.
- Clear capability reporting at startup (`src/mosquitto.c:331-333`) and clear failure logs (`otel_metrics__classify` → warning/error with HTTP code).
- Endpoint credentials are sanitized before logging (`src/otel_metrics.c:652`), so added config does not leak secrets into operator logs.
- Config changes require a restart and this is logged explicitly rather than silently ignored (`otel_metrics__reload`, `src/otel_metrics.c:781-790`).
- Minor friction: `config__read_file_core` (the parse site, `src/conf.c:1003-2974`) remains monolithic — contributors adding keys must navigate ~2000 lines.

## Requirements alignment
- Matches spec §5.4 flag contract: single `WITH_OTEL` flag, default OFF, dependency discovered only when enabled, capability reported (`spec.md:246-256`).
- Matches D11 zero-overhead gate and D6 metric-mapping/name derivation (asserted in `test/unit/otel/otel_metrics_test.c:105-136`).
- C3 trace-context element correctly ships **documentation + tests only, no broker code** (`spec.md:217-219`), consistent with the "no new data path" decision.

## Context consistency
- No contradictory duplicate implementations found; `struct metric` now has a single definition (`src/sys_tree.h:118-126`) reused by both `http_api.c` and `sys_tree.c`.
- One context-loss indicator: unused `otel_metrics__init` (P1).

## Recommended simplifications (top 3)
1. **Delete `otel_metrics__init`** (P1) — removes dead declaration + definition; ~4 LOC, no behavior change.
2. **Collapse `otel__report` into the export call** (P2) — optional; ~16 LOC reduction, keeps classification testable.
3. **None required** for the exporter core — it is already minimal for its contract.

## Open questions / unverified
- **Unverified:** whether `otel_metrics__init` is intended to become the future init hook for the deferred OTLP log export (spec §5.4 mentions gRPC/log-export may land under the same flag). If so it is a deliberate placeholder, not dead code.
- **Unverified:** total demo LOC excludes `demo/k3s/services/*/target/` (ignored) — a build output, not source.
