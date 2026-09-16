#!/bin/sh
# demo/scripts/stop-all.sh — stops the demo broker and collector.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
"$root/demo/scripts/stop-broker.sh"
"$root/demo/scripts/stop-collector.sh"
