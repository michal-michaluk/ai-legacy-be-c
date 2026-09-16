#!/bin/sh
# demo/scripts/stop-broker.sh — stops the demo broker if it is tracked.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
pidfile="$root/.agents/tmp/demo-broker.pid"
if [ -f "$pidfile" ]; then
	kill "$(cat "$pidfile")" 2>/dev/null
	rm -f "$pidfile"
	echo "broker stopped"
else
	echo "broker not running"
fi
