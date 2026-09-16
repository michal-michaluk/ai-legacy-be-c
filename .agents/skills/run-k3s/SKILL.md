---
name: run-k3s
description: Run, inspect, stop and rebuild the k3s-hosted MQTT/OpenTelemetry demo stack (k3s + otel-collector + mosquitto WITH_OTEL + two settings microservices publishing MQTT config-change notifications with W3C trace-context propagation). Use when asked to start/stop/restart the k3s demo, check pod/OTel status, view consumer or collector logs, rebuild a service image, or prove span propagation.
---

# Run (k3s + OpenTelemetry + MQTT)

Self-contained skill that boots a **single-instance k3s** (a privileged
container via podman/docker — no k3d) and deploys, inspects, rebuilds and tears
down the MQTT/OTel demo stack. All commands go through `kubectl` against the
cluster's kubeconfig; nothing here is a Docker-only wrapper.

- CLI: `.agents/skills/run-k3s/scripts/run_k3s.py`
- Manifests: `demo/k3s/k8s/` (kustomize)
- Images: `demo/k3s/docker/mosquitto-otel.Dockerfile`, `demo/k3s/services/settings-service/`
- State: `.agents/runtime/k3s-demo.json`; kubeconfig `.agents/runtime/k3s-demo-kubeconfig.yaml`

## What runs (namespace `otel-mqtt-demo`)

| Workload | What | Notes |
| --- | --- | --- |
| `mosquitto` | broker built from this repo with `WITH_OTEL=ON` | OTLP/HTTP metrics push to `otel-collector:4318` |
| `otel-collector` | OpenTelemetry Collector | OTLP receivers (4317/4318); `debug` + `file` + `otlp/jaeger` exporters |
| `jaeger` | Jaeger all-in-one | trace **UI** at `:16686`; the collector exports spans to it over OTLP/gRPC |
| `settings-service-a` / `-b` | two instances of the reference microservice | write a config key every 5s → publish an MQTT 5 notification; both subscribe and receive |
| `postgres` + `db-migrate` | settings store + schema Job | migrations never run in the app (D1/D11) |

### Notification + trace propagation

Each successful config write publishes to `settings/<tenant>/<key>` with the
serialized change as payload and the W3C trace context (`traceparent` /
`tracestate`) as **MQTT 5 User Properties** (the carrier convention in
`docs/specs/kubernetes-native/trace-context-convention.md`). Mosquitto forwards
them unmodified; the receiving instance extracts them and starts a `Consumer`
span child of the producer's, so one trace id spans both services.

## Prerequisites

- A container runtime: `podman machine start` (or Docker Desktop).
- `kubectl` on PATH. `kustomize` is used via `kubectl apply -k`.
- Optional (re)builds need nothing else — the images build in-container.

## Core commands (run from the workspace root)

```sh
K="python3 .agents/skills/run-k3s/scripts/run_k3s.py"

$K deploy                 # boot k3s (idempotent), build+import images, apply stack
$K deploy --skip-build    # use existing images
$K status                 # nodes, pods, services, publish/receive activity
$K logs collector         # otel-collector logs (traces + metrics, debug exporter)
$K logs a --follow        # settings-service-a logs
$K logs mosquitto         # broker logs (OTel export enabled/errors)

$K rebuild mosquitto      # rebuild+import+rollout the broker only
$K rebuild settings-service
$K rebuild all

$K verify                 # assert OTLP metrics + cross-service span propagation
$K ui                     # port-forward + URL for the Jaeger trace UI
$K ui --stop              # stop the port-forward
$K traces --limit 5       # list recent traces (span names) from the Jaeger API
$K restart                # stop + deploy (images kept)
$K stop                   # delete the k3s cluster + kubeconfig
```

All commands are idempotent: `deploy` on a live cluster skips the cluster and
existing images; `stop` on a stopped cluster is a no-op.

## Visualize traces (Jaeger)

The collector's traces pipeline exports to the in-cluster **Jaeger** UI:

```sh
$K ui            # prints http://localhost:16686 (kubectl port-forward)
```

In the UI pick service `settings-service-a` or `settings-service-b` and open a
trace — it contains a `mqtt.publish` (Producer) span and a `mqtt.receive`
(Consumer) span; the same trace id is visible under **both** services because
the MQTT User Properties carry the trace context. `$K traces` prints the same
information from the Jaeger HTTP API, for a quick check without a browser.

## Proven gates

`verify` is deterministic and exits non-zero on failure:

1. `mosquitto` reports `OpenTelemetry support available` (built `WITH_OTEL`).
2. the collector received `mosquitto_*` metrics over OTLP (C2).
3. a single trace id carries **both** `mqtt.publish` and `mqtt.receive` spans
   (C3 trace-context propagation across the two services).
4. both `settings-service-*` pods logged a received notification with a
   `traceparent` User Property.

## Scope / boundaries

- **In scope:** local single-instance k3s, the three demo layers, their images.
- **Out of scope:** production clusters, ingress/TLS, authentication (the demo
  loop writes through the service port; Keycloak is intentionally absent).
- Mosquitto is consumed **as-is**; the demo must not patch broker behaviour. Any
  broker-side issue is reported in
  `docs/specs/kubernetes-native/validation-report.md`, not worked around here.
