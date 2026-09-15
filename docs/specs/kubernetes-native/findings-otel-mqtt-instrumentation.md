# Findings — OpenTelemetry MQTT instrumentation landscape (D2 research)

What exists today for MQTT trace-context propagation across client ecosystems/languages. Cited.

**Baseline convention:** W3C *Trace Context: MQTT* — `traceparent`/`tracestate` in MQTT 5 **User
Properties** (MQTT 3: outermost JSON payload / binary). Repo README now says **"DISCONTINUED"**, the
doc still declares status "Standard"
([20-MQTT5_FORMAT.md](https://github.com/w3c/trace-context-mqtt/blob/main/spec/20-MQTT5_FORMAT.md),
[README](https://github.com/w3c/trace-context-mqtt)).

## 1. Official OTel instrumentation — nothing, in every language

Verified by recursive git-tree path scan + code search:

| Repo | Result |
|---|---|
| opentelemetry-java-instrumentation | not found — only `camel-2.20` decorators mention mqtt/paho, and their tests assert *"Camel MQTT cannot propagate context"* / *"Paho MQTT cannot propagate context"* ([test](https://github.com/open-telemetry/opentelemetry-java-instrumentation/blob/main/instrumentation/camel-2.20/javaagent-unit-tests/src/test/java/io/opentelemetry/javaagent/instrumentation/camel/v2_20/decorators/MessagingPropagationTest.java)) |
| opentelemetry-python-contrib | nothing found |
| opentelemetry-go-contrib | nothing found |
| opentelemetry-dotnet-contrib | nothing found |
| opentelemetry-js-contrib | nothing found |
| opentelemetry-rust-contrib | nothing found |

## 2. Third-party

- **moscajs/aedes-otel-instrumentation** (Node.js, Experimental, last updated 2023-10): wraps Aedes +
  `mqtt-packet`; emits `mqtt.connect` / `{topic} publish` / `{topic} receive` spans. For MQTT 5 it uses
  `propagation.inject(ctx, packet.properties.userProperties)` / `extract(...)` — i.e. **User Properties**;
  MQTT 3 falls back to JSON payload. Cites the W3C MQTT spec.
  ([repo](https://github.com/moscajs/aedes-otel-instrumentation), [utils.ts](https://github.com/moscajs/aedes-otel-instrumentation/blob/main/src/utils.ts))
- **HiveMQ extensions:** nothing found.
- **SmallRye Reactive Messaging MQTT:** generic tracing hooks exist, but `MqttConnector.java` has **no**
  tracing references → nothing for MQTT ([repo](https://github.com/smallrye/smallrye-reactive-messaging/tree/main/smallrye-reactive-messaging-mqtt)).
- **Spring Integration MQTT:** only `MqttHeaderMapper`; no OTel propagation.
- Misc samples: `shirou/mqttotel`, `OSgAgA/mqtt2otel`, `abicky/opentelemetry-collector-mqtt`.

## 3. Java agent — no

`docs/supported-libraries.md` has no mqtt/paho/hivemq entry; no Paho/HiveMQ client instrumentation module
([supported-libraries.md](https://github.com/open-telemetry/opentelemetry-java-instrumentation/blob/main/docs/supported-libraries.md)).

## 4. Rust — manual only

- OTel works via `opentelemetry` 0.32 + `tracing-opentelemetry` 0.33 (`#[instrument]`)
  ([crates.io](https://crates.io/crates/opentelemetry), [tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry)).
- Propagation via `opentelemetry::propagation::TextMapPropagator` + `TraceContextPropagator`
  ([text_map_propagator.rs](https://github.com/open-telemetry/opentelemetry-rust/blob/main/opentelemetry/src/propagation/text_map_propagator.rs)).
- **No MQTT-specific Rust instrumentation** (rumqttc, paho-mqtt) — you would implement a custom
  `TextMapInjector`/`Extractor` over MQTT 5 user properties.

## 5. Client libraries expose MQTT 5 User Properties — all major ones

| Library | Exposes User Properties |
|---|---|
| Eclipse Paho Java | yes — `MqttProperties.getUserProperties()` ([src](https://github.com/eclipse-paho/paho.mqtt.java/blob/master/org.eclipse.paho.mqttv5.client/src/main/java/org/eclipse/paho/mqttv5/common/packet/MqttProperties.java)) |
| Eclipse Paho C | yes — `MQTTPROPERTY_CODE_USER_PROPERTY = 38` ([header](https://github.com/eclipse-paho/paho.mqtt.c/blob/master/src/MQTTProperties.h)) |
| Eclipse Paho Python 2.1.0 | yes — `Properties.UserProperty` ([properties.py](https://github.com/eclipse-paho/paho.mqtt.python/blob/master/src/paho/mqtt/properties.py)) |
| HiveMQ Java client | yes — `Mqtt5Publish.getUserProperties()` ([src](https://github.com/hivemq/hivemq-mqtt-client/blob/master/src/main/java/com/hivemq/client/mqtt/mqtt5/message/publish/Mqtt5Publish.java)) |
| MQTT.js | yes — `properties.userProperties` on publish/subscribe ([README](https://github.com/mqttjs/MQTT.js)) |

## 6. Summary

| Language | Official | Third-party | Context transport |
|---|---|---|---|
| Java (agent) | none | none | — |
| Java (manual) | none | none | MQTT 5 User Properties |
| Python | nothing found | none | Paho `UserProperty` |
| Go | nothing found | sample only | JSON payload / User Props |
| .NET | nothing found | none | — |
| Node | nothing found | aedes-otel-instrumentation (experimental) | MQTT 5 User Properties |
| Rust | nothing found | none | manual `TextMapPropagator` over User Props |

## Bottom line

- **No official OTel MQTT instrumentation exists in any language.** The only real implementation is the
  experimental Node.js `aedes-otel-instrumentation`, following the (discontinued) W3C MQTT spec.
- For a broker that only **carries** context transparently, the W3C MQTT spec + MQTT 5 User Properties
  is the **only established convention**, and **every major client library can read/write them**.
- Clients own tracing (D2 confirmed by the ecosystem: nothing exists broker-side — consistent with Kafka).
