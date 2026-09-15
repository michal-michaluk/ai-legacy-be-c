# Findings — OpenTelemetry trace-context propagation in messaging brokers (Q4 research)

Web research with primary-source URLs. Question: how do other brokers carry OTel trace context,
and what is the standard for MQTT?

## Kafka

- Kafka record **headers** are generic `String key` → `byte[] value` ([KIP-82](https://cwiki.apache.org/confluence/display/KAFKA/KIP-82+-+Add+Record+Headers)).
- OTel Java instrumentation injects context via a `TextMapPropagator` (`KafkaPropagation.propagateContext` → `KafkaHeadersSetter`, [KafkaPropagation.java](https://github.com/open-telemetry/opentelemetry-java-instrumentation/blob/main/instrumentation/kafka/kafka-clients/kafka-clients-common-0.11/library/src/main/java/io/opentelemetry/instrumentation/kafkaclients/common/v0_11/internal/KafkaPropagation.java)).
- Default propagator is W3C, so the written headers are literally **`traceparent`** and **`tracestate`** ([W3CTraceContextPropagator.java](https://github.com/open-telemetry/opentelemetry-java/blob/main/api/all/src/main/java/io/opentelemetry/api/trace/propagation/W3CTraceContextPropagator.java)).
- **Kafka brokers are transparent** — they store/forward headers; they do not inject OTel spans.

### Broker-side spans (dedicated research)

**No Kafka broker emits OTel spans — tracing is entirely client-side.**

- Strimzi: "The Apache Kafka project itself doesn't have support for tracing, so when messages flow through brokers no further tracing information is added." ([strimzi.io](https://strimzi.io/blog/2023/03/01/opentelemetry/)).
- The only Kafka KIP touching OTel is **KIP-714 (Client metrics and observability)** — OTLP as the *metrics* wire format; it states **"Tracing and logging are outside the scope of this proposal."** ([KIP-714](https://cwiki.apache.org/confluence/display/KAFKA/KIP-714:+Client+metrics+and+observability)).
- Instrumentation lives only on the **clients** (`instrumentation/kafka/kafka-clients`, [tree](https://github.com/open-telemetry/opentelemetry-java-instrumentation/tree/main/instrumentation/kafka)); blog: instrument clients, never the broker ([opentelemetry.io](https://opentelemetry.io/blog/2022/instrument-kafka-clients/)).
- No broker tracing KIP exists; a broker-with-OTel experiment "didn't produce any data" ([instaclustr](https://www.instaclustr.com/blog/tracing-apache-kafka-with-opentelemetry/)).

**OTel messaging model statement** (*Semantic conventions for messaging spans → Context propagation*):

> "Messaging systems themselves may trace messages as the messages travels from producers to consumers. Such tracing would cover the transport layer but would not help in correlating producers with consumers. To be able to directly correlate producers with consumers, another context that is propagated with the message is required."

> "The message creation context is created by the producer and should be propagated to the consumer(s)."

([opentelemetry.io/docs/specs/semconv/messaging/messaging-spans](https://opentelemetry.io/docs/specs/semconv/messaging/messaging-spans/))

**Span kinds** define only producer/consumer-initiated spans (`create→PRODUCER`, `send→PRODUCER/CLIENT`, `receive→CLIENT`, `process→CONSUMER`, `settle→CLIENT`) — **there is no broker/server span kind**.

**Metrics analogy:** Kafka exposes broker metrics via **JMX** and bridges to Prometheus with [prometheus/jmx_exporter](https://github.com/prometheus/jmx_exporter) ([kafka.apache.org/41/operations/monitoring](https://kafka.apache.org/41/operations/monitoring)); the OTel demo scrapes broker metrics through a JMX module, not a broker API ([demo/services/kafka](https://opentelemetry.io/docs/demo/services/kafka/)).

**Takeaway:** broker **exposes metrics** (JMX→exporter); **tracing is a client responsibility**; broker-side tracing is explicitly *not* the correlation mechanism. This validates C3 (carry context only) and C2 (OTLP metrics export).

**Not documented:** no broker-created OTel span kind; no Kafka broker-tracing KIP; Kafka does not natively emit OTLP traces.

## W3C Trace Context

- `traceparent` version `00`; format `version-trace-id-parent-id-trace-flags`, e.g. `00-0af7…-b7ad…-01` ([W3C §3.2](https://www.w3.org/TR/trace-context/)). No v01.
- Header names exactly `traceparent` / `tracestate`; lowercase recommended, receivers MUST accept any case ([§3.2.1, §3.3.1](https://www.w3.org/TR/trace-context/)).
- OTel default propagators = `tracecontext` + `baggage` ([PropagatorConfiguration.java](https://github.com/open-telemetry/opentelemetry-java/blob/main/sdk-extensions/autoconfigure/src/main/java/io/opentelemetry/sdk/autoconfigure/PropagatorConfiguration.java)).
- Non-HTTP protocols are deferred to extensions — "Other Communication Protocols" / [protocols registry](https://www.w3.org/TR/trace-context/#other-communication-protocols).

## MQTT specifics

- The W3C **`trace-context-mqtt` draft is abandoned**: "This document was abandoned. DO NOT USE." ([repo](https://github.com/w3c/trace-context-mqtt), [spec](https://w3c.github.io/trace-context-mqtt/)).
- De-facto convention it left behind: MQTT 5 **User Properties** named `traceparent` / `tracestate` ([20-MQTT5_FORMAT.md](https://github.com/w3c/trace-context-mqtt/blob/main/spec/20-MQTT5_FORMAT.md)).
- OTel semconv references the W3C AMQP/MQTT drafts but "does not specify the exact mechanisms… Future versions… once those standards reach a stable state".
- No official OTel MQTT instrumentation in [python-contrib](https://github.com/open-telemetry/opentelemetry-python-contrib) or [java-instrumentation](https://github.com/open-telemetry/opentelemetry-java-instrumentation). Third-party [aedes-otel-instrumentation](https://github.com/moscajs/aedes-otel-instrumentation) extracts/injects from `packet.properties.userProperties` ([utils.ts](https://github.com/moscajs/aedes-otel-instrumentation/blob/main/src/utils.ts)).

## OTel messaging semantic conventions

- Attributes: `messaging.system` (required), `messaging.destination.name`, `messaging.operation.name`,
  `messaging.operation.type`, `messaging.message.id`, `messaging.client.id`
  ([messaging-spans.md](https://github.com/open-telemetry/semantic-conventions/blob/main/docs/messaging/messaging-spans.md)).
- Span kinds: `create`→PRODUCER, `send`→PRODUCER/CLIENT, `receive`→CLIENT, `process`→CONSUMER, `settle`→CLIENT; consumer `process` is **linked** to producer creation context.
- `messaging.system` has **no `mqtt` value** (known: activemq, aws.*, eventgrid, eventhubs, gcp_pubsub, jms, kafka, pulsar, rabbitmq, rocketmq, servicebus) — a custom value MAY be used ([kafka.md](https://opentelemetry.io/docs/specs/semconv/messaging/kafka/)).

## Implications for the MQTT broker

1. **Implement a de-facto convention, not a standard** — document it as such.
2. Use MQTT 5 User Properties named exactly `traceparent` / `tracestate`, carrying W3C v00 values, unmodified.
3. Be a **transparent carrier**: forward from inbound PUBLISH to matching outbound PUBLISH; do not parse/rewrite; remove only invalid `traceparent` if at all.
4. **MQTT 5 only** — MQTT 3.1.1 has no metadata channel (payload embedding would require mutating payloads).
5. Client instrumentation must choose a custom `messaging.system` value (no `mqtt` constant exists).
