#!/bin/sh
# Stop all managed apps (tracked + orphans), clear state, then show a snapshot.
# Idempotent: safe to re-run.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

collected=""
for name in $(state_names); do
    collected="$collected $(app_pid "$name") $(orphan_pids "$name")"
done
# Always sweep known app orphans even if state was already cleared.
for name in broker dashboard; do
    collected="$collected $(orphan_pids "$name")"
done

# shellcheck disable=SC2086
collected=$(normalize_pids $collected)

if [ -z "$collected" ]; then
    printf 'No running managed processes found.\n'
else
    printf 'Stopping managed processes (pids:%s)...\n' "$collected"
    # shellcheck disable=SC2086
    stop_pids $collected
fi

for name in $(state_names); do
    state_remove "$name"
done

print_process_snapshot
