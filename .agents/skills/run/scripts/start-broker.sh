#!/bin/sh
# Start the mosquitto broker natively in the background with redirected logs.
# Idempotent: a live broker is left untouched.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

name="broker"

if app_is_alive "$name"; then
    printf '%s already running (pid=%s)\n' "$name" "$(app_pid "$name")"
    exit 0
fi

require_broker_binary || exit $?

write_mosq_conf
log=$(new_log_path "$name")
: > "$log"

printf 'Starting %s: %s -c %s\n' "$name" "$MOSQ_BIN" "$MOSQ_CONF"
( cd "$MOSQ_ROOT" && exec nohup "$MOSQ_BIN" -c "$MOSQ_CONF" ) >"$log" 2>&1 &
pid=$!

state_set "$name" "$pid" "$log" "$MOSQ_ROOT" "$MOSQ_BIN -c $MOSQ_CONF" "$MOSQ_PORT"

deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! is_alive "$pid"; then
        printf 'error: broker exited during startup. Last log lines:\n' >&2
        tail -n 20 "$log" >&2
        state_remove "$name"
        exit 1
    fi
    if grep -q 'mosquitto version .* running' "$log" 2>/dev/null; then
        printf 'broker LIVE pid=%s port=%s\n' "$pid" "$MOSQ_PORT"
        printf 'log: %s\n' "$log"
        exit 0
    fi
    sleep "$SLEEP_TICK"
done

printf 'warning: broker started (pid=%s) but readiness line not seen within 20s\n' "$pid" >&2
printf 'log: %s\n' "$log" >&2
exit 1
