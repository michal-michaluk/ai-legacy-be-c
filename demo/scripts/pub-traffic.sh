#!/bin/sh
# demo/scripts/pub-traffic.sh [count] — subscribes, then publishes MQTT 5 messages
# so both the received and sent on-event counters in the collector output move.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
pub="$root/.agents/tmp/build-otel-on/client/mosquitto_pub"
sub="$root/.agents/tmp/build-otel-on/client/mosquitto_sub"
port="${DEMO_MQTT_PORT:-18833}"
count="${1:-5}"

"$sub" -V mqttv5 -p "$port" -t 'demo/#' -C "$count" > /dev/null 2>&1 &
subpid=$!
sleep 0.5

n=1
while [ "$n" -le "$count" ]; do
	"$pub" -V mqttv5 -p "$port" -t demo/metrics -m "payload-$n"
	printf 'published demo/metrics payload-%s\n' "$n"
	n=$((n + 1))
done

wait "$subpid" 2>/dev/null || true
printf 'subscriber received %s messages\n' "$count"
