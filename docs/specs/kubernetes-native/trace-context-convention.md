# Trace-context carry convention

Consumer/provider-facing description of how OpenTelemetry / W3C trace context is
carried through the mosquitto broker. This is the document referenced by spec
§5.3; the broker implements it by generic MQTT 5 User Property forwarding, so
there is **no trace-specific broker code**.

## Status: de-facto convention, not a ratified standard

There is **no ratified W3C or OpenTelemetry mapping of trace context onto MQTT**:

- The W3C `trace-context-mqtt` draft was **abandoned** — "This document was
  abandoned. DO NOT USE." (<https://github.com/w3c/trace-context-mqtt>).
- OpenTelemetry messaging semantic conventions defer non-HTTP protocols to
  extensions and define no MQTT instrumentation.

What remains is a **de-facto convention** inherited from that draft and used by
third-party MQTT instrumentation: MQTT 5 User Properties named `traceparent`
and `tracestate`. Treat it as a convention, never as a standard. Details and
primary sources: `findings-otel-trace-research.md`.

## The convention

| Aspect | Value |
| --- | --- |
| Carrier | MQTT 5 **User Properties** (property identifier `0x26`, `USER_PROPERTY`) |
| Property names | exactly `traceparent` and `tracestate` (lowercase) |
| `traceparent` value | W3C Trace Context **version `00`**: `version-trace-id-parent-id-trace-flags`, e.g. `00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01` (<https://www.w3.org/TR/trace-context/#traceparent-header>) |
| `tracestate` value | W3C Trace Context §3.3 list, e.g. `vendor1=value1,vendor2=value2` (<https://www.w3.org/TR/trace-context/#tracestate-header>) |
| Protocol | **MQTT 5 only** — MQTT 3.1.1 has no metadata channel |
| Producer | injects the User Properties on the PUBLISH it sends |
| Consumer | extracts the User Properties from the PUBLISH it receives |

The broker is a **transparent carrier**: it copies inbound-PUBLISH User
Properties to every matching outbound PUBLISH **unmodified**. It does not parse,
rewrite, inject, remove, reorder or deduplicate them. The broker creates **no
spans**; message-creation context is entirely a client responsibility (the
Kafka/messaging model).

Key case: producers write the exact lowercase keys. W3C's "receivers MUST accept
any case" rule applies to HTTP header names; MQTT User Property keys are opaque,
case-sensitive strings, so consumer-side case tolerance here is **convention
only, unverified** against any ratified MQTT source.

## What the broker guarantees

User Properties live on the shared message property set
(`base_msg->data.properties`) and are re-serialized on every outbound MQTT 5
PUBLISH. Forwarding is complete, with order and duplicates preserved, across:

- normal publish to an MQTT 5 subscriber;
- QoS 0, 1 and 2;
- retained messages, delivered to a subscriber that connects later;
- queued messages for a persistent session, including across a broker restart;
- Will messages, when published;
- shared subscriptions (identical to normal subscriptions);
- WebSocket transports;
- bridges whose link is MQTT 5.

Per-path evidence (file:line) is in `findings-trace-passthrough.md` and spec §5.3.

## Inherent drops (documented, not worked around)

| Drop | Cause |
| --- | --- |
| An **MQTT 3.1.1** subscriber receives **no** User Properties | MQTT 3.1.1 has no property block; the whole property block is gated on the subscriber negotiating MQTT 5 (`lib/send_publish.c`) |
| A message exceeding the subscriber's negotiated `maximum_packet_size` is dropped **in full** — not property-by-property | the outbound PUBLISH returns `MOSQ_ERR_OVERSIZE_PACKET` and the broker drops the whole message (`lib/send_publish.c`, `src/database.c`) |

There is **no per-string truncation** in the forward path: the whole property
set is sent, or the whole message is dropped. Persistence nuance: core
persistence has no Will chunk, so a Will survives restart only via the persist
plugin — out of scope for the carry convention.

## Conformance

The convention is proven by the `broker-25-trace-context-*` E2E scenarios
(`mosquitto/test/broker/25-trace-context-*.py`), covering gates G-C3.1–G-C3.5 of
spec §6:

| Gate | Scenario |
| --- | --- |
| G-C3.1 happy | `25-trace-context-happy.py` — byte-identical `traceparent`/`tracestate` |
| G-C3.2 paths | `25-trace-context-qos.py`, `-retained.py`, `-persistent.py`, `-will.py`, `-shared.py`, `-websocket.py`, `-bridge.py` — QoS 0/1/2; retained; persistent session across restart; Will; two shared groups; WebSockets both directions; bridge local→remote and remote→local |
| G-C3.3 negative | `25-trace-context-v311-negative.py` — MQTT 3.1.1 receives no properties |
| G-C3.4 oversize | `25-trace-context-oversize.py` — whole-message drop, not property-specific |
| G-C3.5 regression | `25-trace-context-regression.py` — fails if core mutates/strips/adds/reorders User Properties, including duplicate keys |

Run locally:

```sh
cd mosquitto/build-tests && ctest -R 'trace-context' --output-on-failure
```
