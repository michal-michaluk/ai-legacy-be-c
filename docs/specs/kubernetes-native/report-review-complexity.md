# Review report — Complexity
- Scope: uncommitted changes (mosquitto submodule + workspace)
- Method: `/Users/michal/.agents/skills/review/references/review-complexity/instructions.md` + vendored `lizard` via `scripts/run-lizard.py`
- Evidence base: `lizard -l c -C 10` on `src/otel_metrics.c`; `-C 15` on `src/sys_tree.c`, `src/http_api.c`, `src/conf.c`; `git diff`, `wc -l`, source reads. Machine-computed CCN/NLOC, no guessing.

## Verdict
New code is **well-factored** (avg CCN 4.6 in `otel_metrics.c`, no function over CCN 13). The real complexity risk is structural: the 4 new config keys were added to `config__read_file_core`, a ~1972-line function with ~172 `else if` branches that **lizard cannot parse**, so its CCN is unmeasurable by the tool.

## Method notes / caveats
- `lizard -l c` reports `src/otel_metrics.c`: 29 functions, avg NLOC 19.8, **avg CCN 4.6**, 3 warnings at CCN>10.
- `lizard` did **not** emit `config__read_file_core` (`src/conf.c:1003-2974`) — its function list stops at `config__plugin_add_secopt@1000`. The tool therefore **under-reports** `conf.c` (reported 18 functions / 2551 nloc). The function's true CCN is unverified.
- `lizard` supports C; no LLM-derived numbers used below. All CCN/NLOC are tool output.

## Findings
| ID | Severity | Finding | Evidence (file:line) | Recommendation |
|---|---|---|---|---|
| C1 | High | Four new `otlp_*` keys were appended to `config__read_file_core`, a ~1972-line function containing ~172 `else if` branches. This grows the single largest complexity hotspot in the file, and lizard cannot parse it, so its CCN is untracked. | `mosquitto/src/conf.c:1003-2974`; new branches `:2345-2372` | Extract the `#ifdef WITH_OTEL` key handling into a helper (e.g. `config__parse_otel_key`) called from the chain; add it to the complexity gate. |
| C2 | Medium | `otel_metrics__encode_json` has the highest CCN in the new code (13, NLOC 75, tokens 493), driven by 5 nested cJSON object creations per sample in one loop. | `mosquitto/src/otel_metrics.c:188-270` | Optionally split per-metric encoding into a helper to drop CCN below 10 and remove the nested-error cleanup ladder. Correctness is otherwise sound. |
| C3 | Low | `otel_metrics__start` CCN 12 / NLOC 47 and `otel__config_equal` CCN 11 / NLOC 24 exceed the CCN>10 warning threshold in the new file (3 warnings total). | `mosquitto/src/otel_metrics.c:680-729`, `:754-778` | `otel__config_equal` can use a small per-header compare helper; `start` is mostly linear init — acceptable but flagged. |
| C4 | Info | `otel__export_once` CCN 9 / NLOC 49 and `otel__post` CCN 8 / NLOC 52 are the next-largest new functions; total new-file warning ratio 3/29 (10%). | `mosquitto/src/otel_metrics.c:559-612`, `:449-505` | No action; within baseline. |
| C5 | Info | Pre-existing hotspots remain unchanged by this diff: `sys_tree__update` CCN 20, `config__read` CCN 38, `config__parse_args` CCN 32, `http_api__start` CCN 21, `http__canonical_filename` CCN 16. | `mosquitto/src/sys_tree.c:259-340`; `src/conf.c:770-907`, `:575-671`; `src/http_api.c:441-548`, `:59-124` | Not in scope; note only to show the diff does not worsen them (the `sys_tree.c`/`http_api.c` hunks are additive). |
| C6 | Info | Changed-file averages: `otel_metrics.c` avg CCN 4.6; `sys_tree.c` avg CCN 4.9 (9 funcs); `http_api.c` avg CCN 6.1 (15 funcs). No new function exceeds the project's implied CCN gate (warnings start >10, all >15 hotspots are pre-existing). | lizard output, all files above | None. |

## Good practices / strengths
- Pure-function separation: classification table + encoder are I/O-free and unit-tested (`test/unit/otel/otel_metrics_test.c`), which keeps CCN local and testable.
- The new thread body is tiny (CCN 4, `otel_metrics.c:625-647`); concurrency complexity is confined to start/stop, not spread across the export path.
- `config.mk`/CMake keep the flag default OFF, so disabled builds add zero code (verified by `test/otel/zero-overhead.sh`).

## Open questions / unverified
- **Unverified:** true CCN of `config__read_file_core` (lizard could not parse it); the C1 assessment rests on line count (~1972) and branch count (~172), not a computed metric.
- **Unverified:** whether the CI complexity gate (if any) covers C; no arch/complexity config found in the submodule.
