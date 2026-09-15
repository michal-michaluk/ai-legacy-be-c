#!/bin/sh
# Show state for every managed app + a native process snapshot.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

names=$(state_names)
if [ -z "$names" ]; then
    printf 'No managed processes found (state: %s).\n' "$STATE_FILE"
else
    for name in $names; do
        pid=$(app_pid "$name")
        log=$(state_field "$name" logPath)
        port=$(state_field "$name" port)
        epoch=$(state_field "$name" startedAtEpoch)

        if is_alive "$pid"; then
            status="LIVE"
            uptime=$(state_uptime "$epoch")
        else
            status="DEAD"
            uptime="--"
        fi

        printf '%s: %s pid=%s uptime=%s port=%s\n' "$name" "$status" "${pid:-?}" "$uptime" "${port:-?}"
        if [ "$status" = "LIVE" ] && [ -n "${port:-}" ]; then
            if port_listening "$port"; then
                printf 'port: %s listening\n' "$port"
            else
                printf 'port: %s not listening\n' "$port"
            fi
        fi
        printf 'log: %s\n' "${log:-<none>}"
        printf 'last: %s\n\n' "$(last_log_line "${log:-}")"
    done
fi

print_process_snapshot
