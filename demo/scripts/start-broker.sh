#!/bin/sh
# demo/scripts/start-broker.sh <on|off> [otlp-endpoint]
# Starts the demo broker, waits for readiness and prints its capability lines.
# Without the otlp endpoint argument no otlp_* key is written, so the OFF build
# also starts cleanly (it rejects otlp_* keys).
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
mode="$1"
otlp="$2"
port="${DEMO_MQTT_PORT:-18833}"
build="$root/.agents/tmp/build-otel-$mode"
conf="$root/.agents/tmp/demo-broker-$mode.conf"
log="$root/.agents/tmp/demo-broker-$mode.log"

{
	echo "listener $port"
	echo "allow_anonymous true"
	if [ "$mode" = on ] && [ -n "$otlp" ]; then
		echo "otlp_endpoint $otlp"
		echo "otlp_export_interval 1"
	fi
} > "$conf"

: > "$log"
"$build/src/mosquitto" -c "$conf" > "$log" 2>&1 &
echo $! > "$root/.agents/tmp/demo-broker.pid"

i=0
while [ "$i" -lt 50 ]; do
	grep -q 'running' "$log" && break
	sleep 0.1
	i=$((i + 1))
done

printf 'broker [WITH_OTEL=%s] on port %s\n' "$mode" "$port"
grep -E 'OpenTelemetry|running' "$log"
