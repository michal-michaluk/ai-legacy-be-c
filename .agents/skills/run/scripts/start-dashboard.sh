#!/bin/sh
# Start the static mosquitto dashboard (dashboard/src) via python http.server.
# Idempotent: a live server on the configured port is left untouched.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

name="dashboard"

if app_is_alive "$name"; then
    printf '%s already running (pid=%s)\n' "$name" "$(app_pid "$name")"
    exit 0
fi

if [ ! -f "$DASHBOARD_DIR/index.html" ]; then
    printf 'error: dashboard index not found at %s/index.html\n' "$DASHBOARD_DIR" >&2
    exit 3
fi

log=$(new_log_path "$name")
: > "$log"

printf 'Starting %s: %s -m http.server %s (cwd=%s)\n' "$name" "$PY" "$DASHBOARD_PORT" "$DASHBOARD_DIR"
( cd "$DASHBOARD_DIR" && exec nohup "$PY" -u -m http.server "$DASHBOARD_PORT" --bind 127.0.0.1 ) >"$log" 2>&1 &
pid=$!

state_set "$name" "$pid" "$log" "$DASHBOARD_DIR" "$PY -m http.server $DASHBOARD_PORT" "$DASHBOARD_PORT"

deadline=$(( $(date +%s) + 12 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! is_alive "$pid"; then
        printf 'error: dashboard exited during startup. Last log lines:\n' >&2
        tail -n 20 "$log" >&2
        state_remove "$name"
        exit 1
    fi
    if http_ready "http://127.0.0.1:$DASHBOARD_PORT/"; then
        printf 'dashboard LIVE pid=%s url=http://127.0.0.1:%s/\n' "$pid" "$DASHBOARD_PORT"
        printf 'log: %s\n' "$log"
        exit 0
    fi
    sleep "$SLEEP_TICK"
done

printf 'warning: dashboard started (pid=%s) but HTTP readiness not seen within 12s\n' "$pid" >&2
printf 'log: %s\n' "$log" >&2
exit 1
