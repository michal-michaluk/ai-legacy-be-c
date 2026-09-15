#!/bin/sh
# Stop one managed app (tracked pid + orphan processes), then show a snapshot.
# Idempotent: stopping an already-stopped app is a no-op.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

if [ "$#" -ne 1 ]; then
    printf 'usage: %s <broker|dashboard>\n' "$0" >&2
    exit 2
fi
name="$1"

tracked=$(app_pid "$name")
orphans=$(orphan_pids "$name")

# shellcheck disable=SC2086
any=$(normalize_pids $tracked $orphans)

if [ -z "$any" ]; then
    printf "No running process found for app '%s'.\n" "$name"
else
    printf "Stopping app '%s' (pids:%s)...\n" "$name" "$any"
    # shellcheck disable=SC2086
    stop_pids $any
    remaining=""
    for p in $any; do is_alive "$p" && remaining="$remaining $p"; done
    if [ -z "$remaining" ]; then
        printf "Stopped app '%s' successfully.\n" "$name"
    else
        printf "Warning: app '%s' pids still running:%s\n" "$name" "$remaining" >&2
    fi
fi

state_remove "$name"
print_process_snapshot
