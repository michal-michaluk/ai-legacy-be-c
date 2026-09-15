---
name: run
description: Run, check, stop and rebuild the mosquitto broker and its static dashboard as native local processes. Use when asked to start/stop/restart mosquitto, check broker or dashboard status, view run logs, or rebuild-and-restart after a code change.
---

# Run (native processes)

Self-contained skill to build, start, inspect, stop and restart the native
mosquitto broker and the static dashboard locally in the background with
redirected, timestamped logs.

- Scripts: `.agents/skills/run/scripts/`
- Runtime state: `.agents/runtime/processes.json`
- Logs: `.agents/runtime/logs/<app>.<yyyyMMdd-HHmmss>.txt`
- Build logs: `.agents/runtime/logs/build/<app>-<step>.<yyyyMMdd-HHmmss>.log`

## Scope

- **In scope:** native, local process management on **macOS + Linux** (POSIX).
- **Out of scope:** cluster-based (k3s / Kubernetes / Docker) runs. Those are a
  **separate future skill** (e.g. `run-k3s`) and must not be added here. This
  skill never uses `kubectl`, `helm`, `k3s`, `k8s` manifests or compose.

## Managed apps

| App | What | Command | Port | Readiness |
| --- | --- | --- | --- | --- |
| `broker` | mosquitto MQTT broker | `<build>/src/mosquitto -c .agents/runtime/mosquitto.conf` | 1883 | log line `mosquitto version X running` |
| `dashboard` | static UI (`mosquitto/dashboard/src`) | `python3 -m http.server 3000 --bind 127.0.0.1` | 3000 | HTTP 200 on `/` |

`broker` starts first in `start-all`; both starts are non-blocking and return
once the readiness signal is observed.

### Runtime state schema

`.agents/runtime/processes.json` (written atomically by `scripts/state.py`):

```json
{
  "apps": {
    "<app>": {
      "pid": 12345,
      "startedAt": "2026-09-15T06:45:52+0200",
      "startedAtEpoch": 1789447552,
      "logPath": "/abs/path/logs/<app>.<ts>.txt",
      "workingDirectory": "/abs/path",
      "command": "<exact command line>",
      "port": 1883
    }
  }
}
```

`status-processes.sh` reports `LIVE`/`DEAD` from `pid` (`kill -0`) and uptime
from `startedAtEpoch`; `logPath` supplies the last log line.

## Core commands (run from the workspace root)

```sh
S=.agents/skills/run/scripts

# Start
sh $S/start-all.sh                 # broker + dashboard
sh $S/start-broker.sh              # one app
sh $S/start-dashboard.sh           # one app

# Status (live/dead, pid, uptime, port, last log line, process snapshot)
sh $S/status-processes.sh

# Stop
sh $S/stop-process.sh broker       # one app: broker | dashboard
sh $S/stop-all.sh

# Rebuild then restart
sh $S/restart-after-build.sh broker       # cmake --build + stop + start
sh $S/restart-after-build.sh dashboard    # static: stop + start only
sh $S/restart-after-build.sh all          # build broker, stop-all, start-all
```

All commands are idempotent: starting a live app, stopping a stopped app or
re-running status is safe. Ports are configurable via env (`MOSQ_PORT`,
`DASHBOARD_PORT`) and the broker build dir via `MOSQ_BUILD_DIR`.

## Build

`mosquitto/Makefile` deliberately errors on Darwin; use **CMake** everywhere
(recommended in `mosquitto/README-compiling.md`).

```sh
cmake -S mosquitto -B mosquitto/build -G Ninja -DCMAKE_BUILD_TYPE=Release -DWITH_TESTS=OFF
cmake --build mosquitto/build -j
```

Produces `mosquitto/build/src/mosquitto` (broker) and the client tools under
`mosquitto/build/client/`. `restart-after-build.sh broker` runs the build step
and fails fast on a non-zero exit.

## Cross-platform decision

Scripts are **POSIX `sh`** (not PowerShell). Rationale:

- macOS `/bin/sh` and Linux `dash`/`bash` both run them unchanged; this is the
  only option that is first-class on the stated macOS + Linux targets.
- The broker is a native binary on both; only the launcher differs.
- JSON state is read/written by `scripts/state.py` (Python 3, already a test
  dependency) using atomic `os.replace`, so a partial write can never corrupt
  `.agents/runtime/processes.json`.
- Liveness uses `kill -0`; port checks use `lsof` with an `nc -z` fallback;
  HTTP readiness uses `curl`.

**Windows path (documented, not implemented here):** the broker builds with
CMake/Visual Studio (`mosquitto/README-windows.txt`), but these launcher
scripts require a POSIX shell. On Windows use Git Bash or WSL, or start the
processes directly, e.g. `mosquitto.exe -c mosquitto.conf` and
`py -m http.server 3000` in `mosquitto\dashboard\src`.

## Prerequisites

- macOS: `brew install cmake ninja cjson` (Tailwind CSS is prebuilt in
  `dashboard/src/tailwind/styles.css`; `npm -g install tailwindcss@3` only when
  regenerating CSS).
- Linux: see `mosquitto/README-compiling.md` (cmake, ninja/make, gcc,
  `libcjson-dev`, plus optional features).
- `python3` and `curl` on PATH.

## Readiness signals

- **broker:** `mosquitto version 2.1.2 running` in the log and the MQTT port
  listening. Verify end-to-end:
  `build/client/mosquitto_pub -t test/topic -m hi` then
  `build/client/mosquitto_sub -t test/topic -C 1`.
- **dashboard:** `curl -fsS -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/` → `200`.

The dashboard loads as static files. Its data API (`/api/v1/systree`,
`/api/v1/listeners`, served by `src/http_api.c` on a `protocol http_api`
listener) requires **libmicrohttpd**, which is **not installed** in the current
environment, so the dashboard renders but its charts stay empty. Install it
(`brew install libmicrohttpd`) and reconfigure the broker to enable the API;
this is intentionally left out of the default start path.

## Restart after a change

```sh
sh .agents/skills/run/scripts/restart-after-build.sh broker
```

Rebuilds via CMake (log in `logs/build/`), stops the tracked broker plus any
orphan process matching the broker command, then starts it again and waits for
the readiness line.

## Troubleshooting

- **`broker binary not found`** — build first: `restart-after-build.sh broker`,
  or configure `mosquitto/build` as shown above.
- **Port already in use** — another broker/dashboard is running. Check
  `status-processes.sh`; stop stragglers with `stop-all.sh`, or override
  `MOSQ_PORT` / `DASHBOARD_PORT`.
- **Stale `DEAD` state** — `status-processes.sh` reports `DEAD` for a tracked
  pid that no longer exists; `stop-process.sh <app>` clears it.
- **Build fails** — read the newest `.agents/runtime/logs/build/*.log`; the
  command prints the last 20 lines on failure.
- **Process snapshot shows `OTHER`** — only the exact broker binary path or the
  dashboard `http.server <port>` command are marked `MOSQ`; everything else is
  unrelated tooling.
