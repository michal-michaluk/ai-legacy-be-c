#!/bin/sh
# demo/scripts/metrics-summary.sh [jsonl]
# Summarizes the latest OTLP/HTTP JSON batch the collector wrote to JSONL:
# resource attributes, the headline $SYS counters, the exported set size and the
# raw OTLP/JSON shape of one counter.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
f="${1:-$root/.agents/tmp/demo-metrics.jsonl}"

if [ ! -s "$f" ]; then
	echo "no metrics received yet ($f)"
	exit 0
fi

last=$(tail -1 "$f")
batches=$(wc -l < "$f" | tr -d ' ')
total=$(printf '%s' "$last" | jq '[.resourceMetrics[0].scopeMetrics[0].metrics[]] | length')
sums=$(printf '%s' "$last" | jq '[.resourceMetrics[0].scopeMetrics[0].metrics[] | select(.sum)] | length')

printf '\033[1mOTLP/HTTP JSON received by the collector\033[0m  (%s batches)\n' "$batches"
printf 'resource: '
printf '%s' "$last" | jq -r '.resourceMetrics[0].resource.attributes | map("\(.key)=\(.value.stringValue // .value.intValue // .value.boolValue)") | join("  ")'
printf 'scope:    '
printf '%s' "$last" | jq -r '.resourceMetrics[0].scopeMetrics[0].scope | "\(.name) v\(.version)"'
echo
echo 'headline metrics (latest batch):'
printf '%s' "$last" | jq -r '.resourceMetrics[0].scopeMetrics[0].metrics[] | [.name, (if .sum then "Sum" else "Gauge" end), ((.sum.aggregationTemporality // .gauge.aggregationTemporality // 1) | if . == 2 then "CUMULATIVE" else "GAUGE" end), (.unit // "-"), (((.sum // .gauge).dataPoints[0]) | (.asInt // (.asDouble | tostring)))] | @tsv' \
	| awk -F'\t' '
		BEGIN{
			split("mosquitto_broker_messages_received mosquitto_broker_messages_sent mosquitto_broker_publish_messages_received mosquitto_broker_publish_messages_sent mosquitto_broker_uptime", h, " ")
			for(i in h) want[h[i]] = 1
			printf "%-46s %-6s %-11s %-9s %s\n", "metric", "kind", "temporality", "unit", "value"
		}
		$1 in want { printf "%-46s %-6s %-11s %-9s %s\n", $1, $2, $3, $4, $5 }'
printf 'exported set: %s metrics (%s Sum monotonic + %s Gauge) — full set in %s\n' \
	"$total" "$sums" "$((total - sums))" "${f#"$root"/}"
echo
echo 'raw OTLP/JSON for one counter:'
printf '%s' "$last" | jq -c '.resourceMetrics[0].scopeMetrics[0].metrics[] | select(.name == "mosquitto_broker_messages_received")'
