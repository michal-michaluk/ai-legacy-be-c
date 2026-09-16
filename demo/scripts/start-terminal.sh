#!/bin/sh
# demo/scripts/start-terminal.sh [port]
# Serves a real bash terminal over HTTP with ttyd; Playwright records the page.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
port="${1:-7699}"
mkdir -p "$root/.agents/tmp"
ttyd -p "$port" -i 127.0.0.1 -W -w "$root" \
	-t 'fontSize=14' -t 'disableLeaveAlert=true' \
	-t 'theme={"background":"#1e1e2e","foreground":"#cdd6f4","cursor":"#f5e0dc","selectionBackground":"#585b70"}' \
	bash > "$root/.agents/tmp/demo-terminal.log" 2>&1 &
echo $! > "$root/.agents/tmp/demo-terminal.pid"

i=0
while [ "$i" -lt 50 ]; do
	curl -sf -o /dev/null "http://127.0.0.1:$port/" && break
	sleep 0.1
	i=$((i + 1))
done
echo "terminal ready: http://127.0.0.1:$port/"
