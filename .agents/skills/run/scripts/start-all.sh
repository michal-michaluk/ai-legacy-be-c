#!/bin/sh
# Start all managed apps: broker first (data backend), then the dashboard.
# Non-blocking; each start returns once its readiness signal is observed.
set -u

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

"$DIR/start-broker.sh"
rc_broker=$?
"$DIR/start-dashboard.sh"
rc_dashboard=$?

if [ "$rc_broker" -ne 0 ] || [ "$rc_dashboard" -ne 0 ]; then
    printf 'start-all: broker=%s dashboard=%s\n' "$rc_broker" "$rc_dashboard" >&2
    exit 1
fi
exit 0
