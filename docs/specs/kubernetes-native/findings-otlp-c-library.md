# Findings — OTLP C library options (Q1)

Research for the OTel metrics exporter (C2). Mosquitto is **C** (C11); the broker must stay buildable
with the existing toolchain. Every claim cited.

## opentelemetry-cpp

- Project status **Stable** across Logs/Metrics/Traces; latest **v1.29.0**; C++14–23.
  **C is explicitly not a goal** ("supporting the C Programming Language is not a goal")
  ([README](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/README.md)).
- Metrics exporters exist for **both** transports: `otlp_grpc_metric_exporter.h`,
  `otlp_http_metric_exporter.h` ([exporters/otlp](https://github.com/open-telemetry/opentelemetry-cpp/tree/main/exporters/otlp/include/opentelemetry/exporters/otlp)).
- Independent flags, **both default OFF**: `OTELCPP_WITH_OTLP_GRPC`, `OTELCPP_WITH_OTLP_HTTP`
  ([CMakeLists.txt](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/CMakeLists.txt)).
- Required deps ([dependencies.md](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/docs/dependencies.md)):
  - OTLP/HTTP → **protobuf ≥3.21.6, libcurl ≥7.81, nlohmann/json ≥3.10.5, zlib ≥1.2.11**
  - OTLP/gRPC adds **gRPC ≥1.49.2**; protobuf/gRPC pull **abseil**
- Integration: `find_package(opentelemetry-cpp CONFIG REQUIRED COMPONENTS …)`, FetchContent
  (`SOURCE_DIR`/tag), or vcpkg ([INSTALL.md](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/INSTALL.md)).
- Offline vendoring supported (`--recurse-submodules` / FetchContent `SOURCE_DIR` / vcpkg overlay ports).
  Dependency closure is **large**, dominated by grpc/abseil/protobuf
  ([third_party_release](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/third_party_release)).
- **HTTP+protobuf only (no gRPC)** is possible: vcpkg `otlp-http` feature depends only on curl+protobuf
  ([vcpkg.json](https://github.com/microsoft/vcpkg/blob/master/ports/opentelemetry-cpp/vcpkg.json)).

## Alternatives

- **prometheus-cpp** — MIT, C++ ([repo](https://github.com/jupp0r/prometheus-cpp/blob/master/LICENSE)); also vendored by otel-cpp's Prometheus exporter.
- **Hand-roll OTLP/HTTP** — viable: the wire format is one `ExportMetricsServiceRequest` protobuf
  message POSTed over HTTP; needs only **protobuf + an HTTP client (curl)**, no gRPC/abseil.
- No lighter maintained **C-only** OTLP metrics client surfaced in official sources — do not assume one.

## Prometheus side

- Text exposition format current version **0.0.4**; `Content-Type: text/plain; version=0.0.4`
  ([exposition_formats](https://prometheus.io/docs/instrumenting/exposition_formats/)). **No library needed.**
- **OpenMetrics 1.0.0** is a separate spec; only needed for exemplars etc.
  ([OpenMetrics 1.0](https://prometheus.io/docs/specs/om/open_metrics_spec/)).

## OTLP protocol

- gRPC default port **4317**; HTTP default **4318** ([OTLP spec](https://opentelemetry.io/docs/specs/otlp/)).
- Metrics HTTP path **`/v1/metrics`**, body `ExportMetricsServiceRequest`.
- Binary protobuf primary (`application/x-protobuf`); JSON allowed (`application/json`).
- otel-cpp HTTP default protocol is **`http/protobuf`** (binary), not JSON
  ([otlp_environment.cc](https://github.com/open-telemetry/opentelemetry-cpp/blob/main/exporters/otlp/src/otlp_environment.cc)).

## Licensing

- otel-cpp/gRPC/abseil/opentelemetry-proto = Apache-2.0; protobuf = BSD-3-Clause; curl = MIT/X;
  zlib = zlib; nlohmann-json = MIT; prometheus-cpp = MIT.
- Mosquitto = **EPL-2.0 OR BSD-3-Clause (EDL-1.0)**
  ([LICENSE.txt](https://github.com/eclipse-mosquitto/mosquitto/blob/master/LICENSE.txt)).
- All deps permissive/Apache — mutually compatible; no copyleft in the closure.

## Implications for C2 (decision needed)

1. **opentelemetry-cpp pulls C++ into a C broker build** (C is explicitly out of scope for that project).
   This is a real integration cost/risk beyond the dependency size.
2. **Cheaper path:** hand-roll OTLP/HTTP metrics (protobuf + curl) — no gRPC/abseil; gRPC then needs its own stack.
3. gRPC support (your Q3 answer) commits to the heaviest dependency set (grpc + abseil + protobuf).
4. Prometheus (C1) needs no library at all — just emit text 0.0.4 over the existing HTTP server.

Open for decision: **(a)** use opentelemetry-cpp (C++), **(b)** hand-roll OTLP/HTTP (protobuf+curl) for
HTTP and add gRPC separately, or **(c)** scope C2 to HTTP transport only and defer gRPC.
