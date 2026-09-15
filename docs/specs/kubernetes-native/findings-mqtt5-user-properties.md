# Findings — MQTT 5 User Properties & tracing hooks (Q4)

Evidence for the trace-context propagation capability. Paths relative to `mosquitto/`.

## Data flow (inbound → stored → outbound)

```
wire bytes
  -> property__read_all()          lib/property_mosq.c:176   (called from handle_publish.c)
       -> property__read()         lib/property_mosq.c:35    (USER_PROPERTY at :144-165)
  -> property__process_publish()   src/property_broker.c:144 (copies survivors into base_msg)
  -> struct mosquitto__base_msg    src/mosquitto_broker_internal.h:426-434
       -> data.properties          include/mosquitto/broker.h:97
  -> db__message_store()           src/database.c:986
  -> delivery: db__message_write_inflight_out_single()  src/database.c:1352
       -> send__publish()          lib/send_publish.c:44  -> property__write_all() :319
```

## Parsing

- `property__read_all(int command, struct mosquitto__packet_in *packet, mosquitto_property **properties)` — `lib/property_mosq.c:176`.
- `MQTT_PROP_USER_PROPERTY` case — `lib/property_mosq.c:144-165`; reads name then value, `property_type = MQTT_PROP_TYPE_STRING_PAIR`.
- Incoming PUBLISH: `handle__publish()` `src/handle_publish.c:266` → `property__read_all(CMD_PUBLISH,…)` `:295` → `property__process_publish()` `:301` (MQTT5 only).

## Storage

- `property__process_publish()` `src/property_broker.c:144` keeps five types for onward transmission
  (`:155-158`): `CONTENT_TYPE`, `CORRELATION_DATA`, `PAYLOAD_FORMAT_INDICATOR`, `RESPONSE_TOPIC`,
  **`USER_PROPERTY`**; re-links them onto `base_msg->data.properties` (`:160-167`), preserving order.
- Stored on `struct mosquitto__base_msg` (`src/mosquitto_broker_internal.h:426-434`); created in
  `db__message_store()` `src/database.c:986`. Will equivalent: `property__process_will()`
  `src/property_broker.c:78` (user props at `:94-96`). CONNECT props are **not** retained (`:32`).

## Outbound

- `db__message_write_inflight_out_single()` `src/database.c:1352` reads `base_msg->data.properties`
  `:1391` and passes it to `send__publish()` for QoS 0/1/2 `:1395,1404,1417`.
- `send__publish()` `lib/send_publish.c:44` → `property__write_all(packet, store_props, false)` `:319`.

## Limits / dropped properties

- **No explicit user-property count limit**; bounded only by packet `remaining_length` / `maximum_packet_size`.
- Unknown property IDs are **rejected** (`MOSQ_ERR_MALFORMED_PACKET`), not dropped — `lib/property_mosq.c` default case.
- Duplicate identifiers rejected by `mosquitto_property_check_all()` (`libcommon/property_common.c:670`,
  loop `:713-721`) **except `MQTT_PROP_USER_PROPERTY`** (`:714`) — repeated keys allowed and preserved.
- Value lengths bounded by `uint16` (MQTT string-pair encoding).

## Existing user-property keys read by the broker

- **None in the broker core (`src/`).** The only property read by key is auth method
  (`MQTT_PROP_AUTHENTICATION_METHOD`, `src/handle_connect.c:1132`, `src/handle_auth.c:91`);
  `mosquitto_property_read_string_pair` is not called anywhere in `src/`.
- API exists: `mosquitto_property_read_string_pair()` `libcommon/property_common.c:965`.
- Key conventions exist only in examples/tests (`plugins/examples/message-timestamp/…:60`,
  `plugins/examples/add-properties/…:67,77,87`, `test/broker/c/auth_plugin_v5.c:68`).

## Tracing / OpenTelemetry

- **No tracing code exists anywhere** (`traceparent`, `tracestate`, `opentelemetry`, `otel` absent).
- The broker **does not create spans**; user properties are the only transport, and the
  injection/extraction points are exactly the parse/store/serialize sites above.

## De-facto convention (from web research — see `findings-otel-trace-research.md`)

- Carry W3C Trace Context v00 in MQTT 5 User Properties named exactly **`traceparent`** and
  **`tracestate`** (lowercase), unmodified.
- The broker is a **transparent carrier**: forward them from inbound PUBLISH to matching
  outbound PUBLISH; never rewrite valid values.
- The W3C `trace-context-mqtt` draft is **abandoned** — this is a de-facto convention, not a ratified standard.
- MQTT 3.1.1 has no metadata channel ⇒ propagation is **MQTT 5 only**.
