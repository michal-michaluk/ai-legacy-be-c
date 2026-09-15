# Intake — Kubernetes-native Mosquitto (English)

> English translation of [intake.md](intake.md). Source of record: [intake-transcript.md](intake-transcript.md) (verbatim transcription).

The goal of the upcoming scope of changes to Mosquitto is to make deployment and operation of the broker in Kubernetes — and in a Kubernetes cluster — as easy as possible.

**1. Prometheus-compatible metrics endpoint.**
Today all metrics are exposed only on system topics. We want to expose the same metrics — nothing more — in a Prometheus-compatible format.

**2. OpenTelemetry support (trace context propagation).**
This is about propagating the trace parent and spans: when a client uses messaging based on MQTT 5.0, span information always travels in the User Properties. We want to accept it, store it and propagate it further, so that the full course of distributed tracing is visible — from the client to all listeners, just as in any other messaging system compliant with OpenTelemetry.

**3. OpenTelemetry metrics.**
The same as for Prometheus, but compliant with the OpenTelemetry standard.

**4. All of these features as compile-time opt-in.**
Each of these features must be controlled by a compile-time flag. Everything is opt-in: if someone does not want it, the Mosquitto deployment must stay exactly the same and produce no overhead. OpenTelemetry and Prometheus support can be enabled independently of each other — one, the other, or both.

**5. Structured JSON logging.**
We want to replace Mosquitto's current logging with structured JSON logging, so that logs reach OpenTelemetry and log-collection systems such as Loki or Elasticsearch in a sensible, structured way — which should also make operating the broker easier.
The same opt-in rule applies to logging: if someone does not want it, there is no trace of it afterwards.
