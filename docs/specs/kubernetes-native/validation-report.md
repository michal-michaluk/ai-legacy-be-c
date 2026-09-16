# Validation report — k3s MQTT / OpenTelemetry demo

Status: **working end-to-end on a local single-instance k3s**; span propagation
and OTLP metrics push are proven by the run-k3s skill's `verify` gate.

This report records what was built, the evidence, and every issue encountered.
Mosquitto is consumed as-is: **no broker change was made** (see F1).

## 1. Deliverables

| Artefact | Path |
| --- | --- |
| run-k3s skill (CLI) | `.agents/skills/run-k3s/SKILL.md`, `scripts/run_k3s.py` |
| Mosquitto OTel image | `demo/k3s/docker/mosquitto-otel.Dockerfile` (+ `.dockerignore`) |
| Settings microservice + MQTT | `demo/k3s/services/settings-service/` (copy of the `microservice-rust` example + `src/notify/`) |
| Cluster manifests | `demo/k3s/k8s/` (kustomize; includes `15-jaeger.yaml`) |

Namespace `otel-mqtt-demo` runs: `mosquitto` (WITH_OTEL), `otel-collector`,
`jaeger`, `settings-service-a`, `settings-service-b`, `postgres`, `db-migrate`.

## 2. What the MQTT addition does

Added the `notify` slice to the reference service (the settings core stays
framework-free):

- `src/notify/notifier.rs` — `SettingsNotifier` port + `MqttNotifier` (rumqttc,
  MQTT 5) + `NotifyingSettingsService` decorator. Every successful
  `put`/`patch`/`delete` emits a `ConfigChange` and publishes it to
  `settings/<tenant>/<key>`.
- `src/notify/otel.rs` — OTLP/HTTP span export (`<OTEL_HTTP_ENDPOINT>/v1/traces`)
  and W3C `traceparent`/`tracestate` inject/extract over MQTT 5 **User
  Properties**.
- `src/notify/runtime.rs` — MQTT client/event loop; the loop extracts the trace
  context from each delivered PUBLISH and starts a `Consumer` span child of the
  producer's, so one trace spans both services.

Each instance writes its own config key every 5 s (`DEMO_INTERVAL_SECS`); both
subscribe to `settings/#`, so they receive and continue each other's traces.

## 3. Evidence (first-hand)

`run-k3s verify` → **VERIFY PASSED**:

```
[ok] mosquitto built WITH_OTEL
[ok] collector received 52 mosquitto_* metrics (OTLP push)
[ok] span propagation: 4 trace(s) carry both mqtt.publish and mqtt.receive, e.g. c06961de0472d231cbda73453107121a
[ok] settings-service-a received a notification with a traceparent
[ok] settings-service-b received a notification with a traceparent
```

Broker (pod log): `OpenTelemetry support available` /
`OpenTelemetry metrics export enabled: http://otel-collector:4318 every 2 seconds`;
both clients connect as MQTT 5 (`p5`).

Service (`settings-service-a`) received the peer's change with the carried trace:

```
topic=settings/acme/demo/settings-service-b
trace_id=90ca608f8ccf85505a9f8a7fdd4185c7
traceparent=00-90ca608f8ccf85505a9f8a7fdd4185c7-94ce4436ab4ecd0d-01
payload={"service":"settings-service-b","op":"upsert",...}
```

Collector (debug exporter) shows both `mqtt.publish` (Producer) and
`mqtt.receive` (Consumer) spans under one trace id, plus `mosquitto_*` metrics.

## 4. Trace visualization (Jaeger)

Traces are exported by the collector's `otlp/jaeger` exporter to the in-cluster
**Jaeger all-in-one** (`jaegertracing/all-in-one:1.76.0`), whose UI is served on
port 16686.

```sh
python3 .agents/skills/run-k3s/scripts/run_k3s.py ui      # http://localhost:16686
python3 .agents/skills/run-k3s/scripts/run_k3s.py traces  # same data, from the API
```

`ui` port-forwards `svc/jaeger`; the UI answered `HTTP 200`. The Jaeger API
(`/api/services`) reports `settings-service-a` and `settings-service-b`, and
recent traces carry both operation names, e.g. trace `82a5a23cb64ca449`:

```
settings-service-b: ['mqtt.publish', 'mqtt.receive']
settings-service-a: ['mqtt.publish', 'mqtt.receive']
```

The **same trace id** appears under both services — the visible proof that the
W3C trace context survives the broker round-trip through MQTT User Properties.
(Jaeger also self-instruments its own UI requests as service `jaeger-all-in-one`;
that is noise, not demo traffic.)

## 5. Issues encountered

### F1 — Mosquitto `cmake --install` is a no-op (NOT fixed)

`mosquitto/CMakeLists.txt:101-105` defines `function(custom_install)`, which
forwards `${NARGS}` — a variable defined **only inside `macro`s**, not
`function`s. The `install(${NARGS})` therefore expands to `install()` and the
whole install tree is empty (`install_manifest.txt` is empty; `/stage/usr` is
never created).

- Impact: any CMake-based packaging of mosquitto (including a naive Dockerfile)
  installs nothing.
- **Not fixed** (per instruction). Worked around in the demo Dockerfile by
  copying build artefacts directly (`demo/k3s/docker/mosquitto-otel.Dockerfile`).

### F2 — Rootless podman cannot host k3s (environment, not mosquitto)

The default `podman-machine-default` is rootless; k3s' kubelet refuses to start:

```
failed to run Kubelet: failed to create kubelet: open /dev/kmsg: operation not permitted
... running in UserNS, Hint: enable KubeletInUserNamespace feature flag
```

The pre-existing `settings-k3s` container (11 days old) failed the same way
(`Failed to start ContainerManager: mkdir /sys/fs/cgroup/kubepods: permission denied`).

- Fix: run the cluster on the machine's **rootful** connection
  (`podman --connection podman-machine-default-root`). The CLI auto-selects it
  when the default is rootless (override `RUN_K3S_PODMAN_CONNECTION`).
- Reversible; the user's default podman mode was left unchanged.

### F3 — Local image-name mismatch on import

`podman save` tags locally-built images as `localhost/<name>:dev`, while a
manifest referencing `<name>:dev` is normalized to `docker.io/library/<name>:dev`
→ `ErrImagePull`. Fixed by referencing `localhost/mosquitto-otel:dev` and
`localhost/settings-service-mqtt:dev` in the manifests.

### F4 — Running image predated a log-line edit

The first settings-service image was built before a publish-side log line was
added, so the running image lacked it (functionality unaffected). Refreshed via
`run-k3s rebuild settings-service`, which also validated the rebuild path.

### F5 — Blocking span flush starved the MQTT keepalive (fixed in the service)

`handle_publish`/`notify` called `otel::flush()` inline on the event-loop task.
The blocking export stalled the loop past the keepalive, so the broker dropped
the session (`Mqtt state: Connection closed by peer abruptly` → reconnect every
~4 s). Fix: no flush on the hot path; a separate task flushes every 3 s via
`spawn_blocking`, and keep-alive was raised to 30 s.

### F6 — Duplicate client id during rollout (operational)

Mosquitto logs `session taken over` while a `RollingUpdate` briefly runs the old
and new pod of the same instance with the **same** `MQTT_CLIENT_ID`. Cosmetic
for the demo (self-heals once the old pod terminates) but worth noting: a
production deployment should derive the client id from the pod name
(`metadata.name`) to avoid the takeover churn.

### F7 — Subscription dropped after restart (fixed in the service)

Subscribing once before the event loop is not robust across reconnects. Fix:
subscribe on every `ConnAck` (the canonical rumqttc pattern) in the event loop.

## 6. Command matrix (all exercised)

| Command | Result |
| --- | --- |
| `deploy` / `deploy --skip-build` | PASS (boots k3s, imports images, applies) |
| `status` | PASS (nodes, pods, svc, publish/receive counts) |
| `logs collector` | PASS |
| `rebuild mosquitto` | PASS |
| `rebuild settings-service` | PASS (rebuild + import + rollout) |
| `restart` | PASS (stop + deploy) |
| `stop` | PASS (removes cluster + kubeconfig) |
| `verify` | PASS after deploy, after rebuild, and after restart |
| `ui` / `traces` | PASS (Jaeger UI `HTTP 200`; traces listed with both spans) |

## 7. Environment deltas applied (reversible)

- `podman machine start` (was stopped).
- Used `podman-machine-default-root` connection for the cluster only.
- Transferred four images rootless→rootful (`save`/`load`) to avoid re-pulls.

## 8. Reproduce

```sh
podman machine start
python3 .agents/skills/run-k3s/scripts/run_k3s.py deploy      # ~builds images first time
python3 .agents/skills/run-k3s/scripts/run_k3s.py verify
python3 .agents/skills/run-k3s/scripts/run_k3s.py status
python3 .agents/skills/run-k3s/scripts/run_k3s.py stop
```
