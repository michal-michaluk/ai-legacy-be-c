# Findings — MQTT 5 User Property pass-through (Q5)

Question: do MQTT 5 User Properties (`traceparent`/`tracestate`) survive **every** delivery path,
i.e. does trace-context propagation (C3) need any broker code? Paths relative to `mosquitto/`.

Properties are `MQTT_PROP_USER_PROPERTY` (`MOSQ_PROP_TYPE_STRING_PAIR`) on the shared
`base_msg->data.properties`, re-serialized on every outbound PUBLISH for MQTT 5 clients.

## Delivery-path table

| # | Path | Preserved? | Evidence (file:line) |
|---|---|---|---|
| 1 | Normal publish → MQTT 5 subscriber | Yes | keep `src/property_broker.c:159-174`; `src/handle_publish.c:290`; `src/subs.c:113`; `src/database.c:1391`; `lib/send_publish.c:317-333` |
| 2 | QoS 0 / 1 / 2 | Yes — identical | `src/database.c:1395,1404,1417` (same props pointer) |
| 3 | Retained (delivered on later subscribe) | Yes | store `src/retain.c:171-176`; delivery `src/retain.c:283` |
| 4 | Persistent sessions / queued across restart | Yes | `src/persist_write.c:143`; `src/persist_write_v5.c:150-190` (`property__write_all(..., true)` writes all incl. USER_PROPERTY); restore `src/persist_read_v5.c:228-235` → `src/persist_read.c:315` |
| 5 | Will messages | Yes (to subscribers) | `src/property_broker.c:94-114`; `src/context.c:234-241`; `src/database.c:903-905` |
| 6 | Bridges | Yes — **if the bridge link is MQTT 5** | routed like any client: `db__message_insert_outgoing` → `send__publish`; only topic remap `lib/send_publish.c:130-180`; bridge CONNECT props don't touch payload props (`src/bridge.c:399-461,488-644`) |
| 7 | Shared vs normal subscriptions | No difference | `src/subs.c:100-121` |
| 8 | MQTT 3.1.1 subscriber | **No — dropped (expected)** | `lib/send_publish.c:317` gates the whole property block on `mosq->protocol == mosq_p_mqtt5` |
| 9 | WebSockets | No difference | `src/websockets.c:344` → same `handle__packet(mosq)` |
| 10 | Core adding/removing USER_PROPERTY | **No core mutation** | only plugins (`src/plugin_message.c:69-74`, `lib/send_publish.c:69-115`); `add-properties` is an example plugin, not core |

## Size / packet-limit behaviour

- Property block > 128 MB encoded (varint > 4 bytes): `lib/send_publish.c:285-291` silently sets
  `store_props = NULL` → drops **all** properties. Not reachable by two short trace strings.
- Total packet exceeds the subscriber's negotiated `maximum_packet_size`: `lib/send_publish.c:293`
  returns `MOSQ_ERR_OVERSIZE_PACKET`; `src/database.c:1396/1405/1417` **drops the whole message**.
  Realistic loss path when a subscriber advertises a small max packet size. Limit from CONNECT
  `MAXIMUM_PACKET_SIZE` (`src/property_broker.c:53-58`) / `bridge_max_packet_size` (`src/bridge.c:273,511`).
- Incoming PUBLISH exceeds broker `max_packet_size` (default 2,000,000, `src/conf.c:338`):
  `lib/packet_mosq.c:400-444` disconnects the publisher before property parsing. `message_size_limit`
  checks only `payloadlen` (`src/handle_publish.c:334`).
- No per-string truncation exists in the forward path — the whole property set is sent, or the whole message is dropped.

## Verdict (closes Q5)

Forwarding is **complete** across normal, retained, queued/persistent, will, shared, bridge (MQTT 5)
and websocket delivery, on QoS 0/1/2, with order and duplicates preserved. Only inherent, expected
drops: an MQTT 3.1.1 subscriber gets no properties; an oversize message is dropped as a whole.

⇒ **C3 needs no broker code** — it reduces to **documentation + a conformance test** (MQTT 5
subscriber; note the `maximum_packet_size` caveat).

Persistence nuance to document, not code around: core persistence has **no will chunk**
(`src/persist.h:29-32` — no `DB_CHUNK_WILL`); wills survive restart only via the persist plugin
(`src/plugin_persist.c:397-421`). Out of scope for C3.
