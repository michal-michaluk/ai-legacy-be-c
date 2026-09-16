#!/bin/sh
# Resets scratch state for a fresh demo take.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
tmp="$root/.agents/tmp"
mkdir -p "$tmp"
rm -f "$tmp/demo-metrics.jsonl" \
	"$tmp/demo-sub5.txt" "$tmp/demo-sub311.txt" \
	"$tmp/demo-broker.pid" "$tmp/demo-collector.pid"
echo "demo workspace ready: $root"
