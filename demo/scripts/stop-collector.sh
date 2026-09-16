#!/bin/sh
# demo/scripts/stop-collector.sh — stops the demo collector if tracked.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
pidfile="$root/.agents/tmp/demo-collector.pid"
if [ -f "$pidfile" ]; then
	kill "$(cat "$pidfile")" 2>/dev/null
	rm -f "$pidfile"
	echo "collector stopped"
else
	echo "collector not running"
fi
