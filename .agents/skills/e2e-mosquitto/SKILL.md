---
name: e2e-mosquitto
description: Runs and maintains the mosquitto broker/lib/app E2E suite (Python scenarios + CTest). Supports scopes all/impacted/lib/broker/apps and modes NEW/FIX. Without args, lists scenarios and proposes the impacted set from repo diffs.
---

# mosquitto E2E Skill

Use this skill for end-to-end (process + protocol) test work in the mosquitto
project under `mosquitto/`. The suite is **not a browser app**: scenarios are
Python 3 programs that start a real `mosquitto` subprocess, drive it over raw
MQTT/TLS/WebSocket/HTTP sockets or CLI subprocesses, assert exact packets /
exit codes, and tear the process down. CTest registers and runs them.

Authoritative docs (read before editing):

- `docs/test/broker-e2e-tests.md` — broker process / MQTT E2E design
- `docs/test/testing-and-build.md` — feature flags, test layers, CTest
- `docs/test/unit-tests.md` — the non-E2E unit layer
- `docs/arch/code-structure.md` — module ownership
- `AGENTS.md` — workspace rules

## Scope boundaries

Allowed edits (test code only):

- Scenarios: `mosquitto/test/broker/*.py`, `mosquitto/test/lib/*.py`,
  `mosquitto/test/client/*.py`, `mosquitto/test/apps/**/*.py`.
- Scenario-local fixtures beside them: `.conf`, `.pwfile`, `.acl`, certs, and
  the C plugin/helper sources under `mosquitto/test/broker/c/`.
- Registration: `mosquitto/test/{broker,lib,client,apps/**}/CMakeLists.txt`
  (`add_python_test` lines), the matching `test.py` test lists, and the
  `Makefile` section targets — only to register a new scenario.
- Unit tests when scope is `unit`: `mosquitto/test/unit/**`.

Forbidden edits:

- Product code: `mosquitto/src/`, `mosquitto/lib/`, `mosquitto/libcommon/`,
  `mosquitto/common/`, `mosquitto/include/`, `mosquitto/client/`,
  `mosquitto/apps/`, `mosquitto/plugins/`.
- CI: `mosquitto/.github/**`.
- Shared harness internals — changing these silently alters every scenario:
  `test/mosq_test.py`, `test/mosquitto_broker.py`, `test/broker_config.py`,
  `test/mqtt_packets.py`, `test/matchers.py`, `test/ptest.py`,
  `test/mosq_paths.py`, `test/resource.json`.

If a fix appears to require product or harness changes, do **not** apply it:
propose it (see Failure handling).

## Test location taxonomy

| Dir | CTest prefix | Subject | Tech |
| --- | --- | --- | --- |
| `test/unit/` | `unit-` | pure broker/lib/libcommon units | C/CUnit, GoogleTest |
| `test/lib/` | `lib-` | public client-library protocol/API | Python + C helpers |
| `test/broker/` | `broker-` | broker over real MQTT/TLS/WS/HTTP | Python |
| `test/client/` | `client-` | `mosquitto_pub/sub/rr` CLI | Python |
| `test/apps/` | `apps-` | `ctrl`, `db_dump`, `passwd`, `signal` | Python + C++ |
| `test/mock/` | — | C++ mocks | GoogleMock |

Broker scenarios live in `test/broker/`; use `test/lib/` when the client
library is the subject, `test/apps/` for a CLI app, `test/client/` for the
core MQTT CLI clients.

## Invocation matrix

- `/e2e-mosquitto`
  - List scenarios and propose the impacted set from current repo diffs. No
    code changes — advisory only.
- `/e2e-mosquitto all`
  - Run the whole registered suite.
- `/e2e-mosquitto impacted`
  - Map changed paths to scenarios (see Impacted mapping) and run that set.
- `/e2e-mosquitto broker` | `lib` | `client` | `apps` | `unit`
  - Run one layer subset via the CTest name prefix.
- Second token `NEW` / `FIX` (uppercase) modifies the run: e.g.
  `/e2e-mosquitto broker NEW`, `/e2e-mosquitto impacted FIX`.

## Modes

Mode token must be exactly `NEW` or `FIX` (uppercase). Anything else
(`new`, `fix`, misspelling) is a usage error: print the accepted grammar and
stop; do not run.

### `NEW`

- Add or expand scenarios for the requested scope (a new `NN-name.py` file)
  plus any scenario-local fixtures.
- Register every new scenario in all three places:
  1. `add_python_test(PY_TEST_NAMES <prefix> <ports> "<file>.py")` in the
     dir's `CMakeLists.txt` — `check_for_missing_tests` fails the build if a
     globbed file is unregistered.
  2. The `test.py` `tests` list `(ports, './<file>.py')`.
  3. The `Makefile` section target (`NN :`).
- Iterate until green; keep edits in test code + fixtures only.

### `FIX`

- Treat failing scenarios as a repair task. Reproduce minimally, fix the
  scenario or its fixture, iterate until green.
- Change test code only. Never patch product code to make a test pass and
  never weaken an assertion to hide a real regression.

## Failure handling and proposal policy

On any failing scenario:

1. Capture the CTest output / standalone stderr — the harness prints the
   broker log on failure (`MosquittoBroker.stop` prints `self.get_log()`).
2. Reproduce the single scenario minimally (single `ctest -R` or standalone
   run with a fixed port).
3. Classify: (a) scenario/fixture defect → fix in test code; (b) genuine
   product regression → report, do **not** edit product code; (c) harness
   defect → report, do not edit shared harness.
4. With no `NEW`/`FIX` mode: analyze and **propose** concrete fixes first;
   apply test-code changes only after confirmation.
5. Never mutate product code, CI, or shared harness without explicit
   instruction.

## Impacted mapping heuristic

Map changed paths (from `git diff`/`git status`) to layers, then to CTest
name filters:

| Changed path | Layer | Run |
| --- | --- | --- |
| `src/**` (broker runtime, handlers, persistence, routing) | broker | `ctest -R '^broker-'` |
| `src/**` ↔ `test/unit/broker/**` | unit | `ctest -R '^unit-'` |
| `lib/**`, `include/mosquitto/**` | lib | `ctest -R '^lib-'` |
| `libcommon/**`, `common/**` | unit | `ctest -R '^unit-'` |
| `client/**` | client | `ctest -R '^client-'` |
| `apps/**` | apps | `ctest -R '^apps-'` |
| `plugins/**` | broker (plugin scenarios) | `ctest -R '^broker-(09|14|15)-'` |
| `dashboard/**` | UI | no CTest layer; report manually |
| `test/**`, `CMakeLists.txt` test registration | own layer | that layer's subset |

Ambiguous or cross-cutting changes (auth, listeners, feature flags, shared
`libcommon`): run the minimal safe set = directly related layer subset **plus**
a connect/protocol smoke subset, e.g.
`ctest -R '^(broker-01-connect|lib-01)-'`.

If a changed behavior has no matching scenario, propose a concrete new
`test/<layer>/NN-<slug>.py` with its registration entries.

## Command catalog

Run CTest from the build tree (`mosquitto/build-tests`). The configured args
(`-j20 --resource-spec-file ../test/resource.json --output-on-failure
--repeat until-pass:5`) come from `CMakeLists.txt:387-392`.

- One-time configure (only if `mosquitto/build-tests/` is absent):
  - `cmake -S mosquitto -B mosquitto/build-tests -G Ninja -DCMAKE_BUILD_TYPE=Debug -DWITH_TESTS=ON -DWITH_BUNDLED_DEPS=ON && cmake --build mosquitto/build-tests`
- List all registered tests:
  - `ctest -N`
- Run the whole suite:
  - `ctest -j20 --resource-spec-file ../test/resource.json --output-on-failure --repeat until-pass:5`
- Run one layer:
  - `ctest -R '^broker-'` / `'^lib-'` / `'^client-'` / `'^apps-'` / `'^unit-'`
- Run a single scenario (exact name):
  - `ctest -R '^broker-01-connect-allow-anonymous$' --output-on-failure`
- Run the impacted set (example, broker):
  - `ctest -R '^(broker-(01-connect|09-plugin|14-dynsec|15-persist))-' --output-on-failure`
- Run one scenario standalone (no CTest; pass a free port; needs
  `BUILD_ROOT` pointing at the build):
  - `cd mosquitto/test/broker && BUILD_ROOT=<abs>/mosquitto/build-tests python3 01-connect-allow-anonymous.py <port>`
- Legacy runners (equivalent, from `mosquitto/`):
  - `make test` (serial), `make ptest` (parallel), `python3 run_tests.py`
  - `run_tests.py` groups: `test/client`, `test/lib`, `test/apps/*`, `test/broker`.

CTest name = `<prefix><file-stem>`; a scenario's port count is its 3rd
`add_python_test` argument and its `test.py` tuple — keep them consistent.

## Quality rules

Derived from the docs' NOGO list:

- **Ports**: never hardcode a listening port in a scenario. Use
  `mosq_test.get_port()` (reads `CTEST_RESOURCE_GROUP_0_PORTS`) and declare
  the required port count in CTest (`RESOURCE_GROUPS "ports:1"`). Standalone
  runs may pass a port argument; it must not be baked into the file.
- **No shared/global broker**: each scenario owns its `mosquitto` process via
  `MosquittoBroker(...)`; never connect to a developer's running broker or a
  fixed global config file.
- **Unconditional cleanup**: wrap the broker in `with MosquittoBroker(...)`;
  on assertion failure the broker must still be stopped and generated
  `.conf`/credentials/certs/sockets removed. Do not leak processes or files.
- **Exact protocol assertions**: assert exact packet bytes, reason codes,
  properties, flags, ordering, HTTP status/body, or CLI exit code/stdout/
  stderr. Never assert merely that the process stayed alive. Do not weaken a
  contract assertion to make a test pass.
- **Public boundary only**: drive MQTT/TLS/WS/HTTP/CLI, never call private
  broker functions from a scenario.
- **Feature gating**: `mosq_test.require_features(["WITH_TLS"])` for optional
  build capabilities; unsupported environments exit `77` (CTest
  `SKIP_RETURN_CODE 77`). A broken setup must fail, never silently skip.
- **No retry of deterministic assertions**: retry only process readiness /
  explicitly eventual behavior (`mosq_test.retry` / CTest
  `--repeat until-pass:5` cover flakiness of startup, not logic).
- **Determinism**: no dependency on remote services or wall-clock time.
- **Scope**: one behavior per scenario; assert the real client-visible
  contract; keep `test/unit/` free of processes/sockets/brokers.

## NULL / empty-value semantics

mosquitto's "NULL/placeholder" equivalents are protocol-level, not UI labels.
Encode and assert them explicitly:

- Zero-length client id with `allow_zero_length_clientid` → broker
  auto-assigns an id (server-generated client identifier property for v5).
- NULL/absent will and NULL/zero-length topics/payloads follow MQTT rules:
  a zero-length topic is invalid, NULL will means "no will".
- Absent MQTT v5 properties vs present-with-empty-length are distinct; use
  `mqtt_packets.gen_*` and assert the exact property set — do not treat an
  empty property list as equivalent to an omitted one.
- Preserve v3.1.1 vs v5 differences in reason codes (`mqtt5_rc`,
  `mqtt4_rc`) rather than collapsing them.

## Generation checklist

Before finishing, confirm the skill can, without prior conversation context:

- run no-arg and return useful impacted guidance,
- run `all`, `impacted`, and a single layer deterministically,
- enforce `NEW`/`FIX` test-only changes with uppercase validation,
- register any new scenario in CMakeLists + test.py + Makefile,
- keep every command above executable as written.
