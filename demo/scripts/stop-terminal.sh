#!/bin/sh
# demo/scripts/stop-terminal.sh — stops the ttyd terminal server.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
pidfile="$root/.agents/tmp/demo-terminal.pid"
if [ -f "$pidfile" ]; then
	kill "$(cat "$pidfile")" 2>/dev/null
	rm -f "$pidfile"
	echo "terminal stopped"
else
	echo "terminal not running"
fi
