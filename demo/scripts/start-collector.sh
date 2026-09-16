#!/bin/sh
# demo/scripts/start-collector.sh — starts otelcol-contrib (OTLP/HTTP JSON -> JSONL).
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
port="${DEMO_OTLP_PORT:-43183}"
out="$root/.agents/tmp/demo-metrics.jsonl"
log="$root/.agents/tmp/demo-collector.log"

rm -f "$out"
MOSQ_OTELCOL_PORT="$port" MOSQ_OTELCOL_OUTPUT="$out" \
	"$root/.agents/tmp/otelcol/otelcol-contrib" \
	--config "$root/mosquitto/test/otel/collector.yaml" > "$log" 2>&1 &
echo $! > "$root/.agents/tmp/demo-collector.pid"

i=0
while [ "$i" -lt 300 ]; do
	nc -z 127.0.0.1 "$port" >/dev/null 2>&1 && break
	sleep 0.1
	i=$((i + 1))
done

if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
	echo "otelcol-contrib listening: OTLP/HTTP JSON 127.0.0.1:$port -> .agents/tmp/demo-metrics.jsonl"
else
	echo "otelcol-contrib failed to listen (see .agents/tmp/demo-collector.log)"
fi
