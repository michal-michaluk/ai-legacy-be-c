# Acceptance scenarios — Kubernetes-native Mosquitto observability

Source of truth: `docs/specs/kubernetes-native/spec.md` (capabilities C2, C3, C4;
§5.2, §5.3, §6 gates). This is a backend/CLI feature — there is no web UI, so every
scenario is demonstrated through the real terminal workflow (broker + local
OpenTelemetry collector + MQTT client CLIs).

Demo recording: `demo/kubernetes-native.chapters.mkv` (chapters match
`demo/chapters.json`). Driver: `demo/record.js` (Playwright screencast over a real
`ttyd` terminal). See `demo/README.md` for the full reproduce recipe.

1. **Opt-in flag reports the OpenTelemetry capability** — the OFF build logs
   `OpenTelemetry support NOT available`; the ON build logs
   `OpenTelemetry support available` plus the export line. *(C4 / G-C4.4)*
   - start the `WITH_OTEL=OFF` broker and show its startup capability lines
   - start the `WITH_OTEL=ON` broker and show its startup capability lines
2. **OTLP metrics reach the collector** — the collector's JSONL output contains
   `mosquitto_*` metrics with the emitted `$SYS` set: `mosquitto_broker_messages_received`
   is a monotonic cumulative `Sum`, resource attributes carry
   `service.name=mosquitto`. *(C2 / G-C2.1, G-C2.2)*
   - start `otelcol-contrib` with the OTLP/HTTP receiver → file (JSONL) exporter
   - start the ON broker pointed at the collector and publish MQTT traffic
   - summarize the JSONL the collector wrote
3. **Trace context carried PUB → SUB byte-identical** — an MQTT 5 subscriber prints
   exactly the `traceparent`/`tracestate` User Properties the publisher sent.
   *(C3 / G-C3.1)*
   - subscribe with `mosquitto_sub -V mqttv5 -F '%P'`
   - publish with `-D publish user-property traceparent … -D publish user-property tracestate …`
4. **MQTT 3.1.1 has no metadata channel** — the same publish is delivered to an MQTT
   3.1.1 subscriber with **no** User Properties. *(C3 / G-C3.3)*
   - subscribe with `mosquitto_sub -V mqttv311 -F '[%P]'`
   - publish the trace-carrying message again
5. **Zero overhead when the flag is off** — the OFF binary exposes no `otel`/`curl`
   symbols and no libcurl dependency, the ON binary exposes both, and enabling the
   flag adds only libcurl. *(C4 / G-C4.1, G-C4.2, G-C4.4)*
   - run `mosquitto/test/otel/zero-overhead.sh` and show all gates PASS
