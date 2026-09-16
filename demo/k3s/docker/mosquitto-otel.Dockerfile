# Mosquitto broker built from THIS repo's source with the OpenTelemetry feature
# (WITH_OTEL=ON). The stock mosquitto/docker/* images build an upstream release
# tarball and therefore never contain src/otel_metrics.c — this image builds the
# in-tree source so the OTLP metrics exporter (C2) is present.
#
# Build context: the `mosquitto/` submodule directory.
#   podman build -f demo/k3s/docker/mosquitto-otel.Dockerfile \
#     --ignorefile demo/k3s/docker/mosquitto.dockerignore \
#     -t mosquitto-otel:dev mosquitto
#
# Artifacts are copied explicitly: mosquitto's `custom_install` is a CMake
# `function` that forwards `${NARGS}` (defined only for macros), so its
# `install()` rules are empty and `cmake --install` is a no-op here.

FROM alpine:3.23 AS builder

RUN apk add --no-cache \
        build-base cmake linux-headers \
        openssl-dev curl-dev cjson-dev cjson-static util-linux-dev

WORKDIR /src
COPY . /src

RUN cmake -B /build -S /src \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/usr \
        -DWITH_OTEL=ON \
        -DWITH_SYS_TREE=ON \
        -DWITH_TESTS=OFF \
        -DWITH_DOCS=OFF \
        -DWITH_WEBSOCKETS=OFF \
    && cmake --build /build -j "$(nproc)"

# --- runtime ---
FROM alpine:3.23

RUN apk add --no-cache \
        libcurl cjson openssl ca-certificates tzdata \
    && addgroup -S -g 1883 mosquitto 2>/dev/null \
    && adduser -S -u 1883 -D -H -h /var/empty -s /sbin/nologin -G mosquitto -g mosquitto mosquitto 2>/dev/null \
    && mkdir -p /mosquitto/config /mosquitto/data /mosquitto/log \
    && chown -R mosquitto:mosquitto /mosquitto

COPY --from=builder /build/src/mosquitto /usr/sbin/mosquitto
COPY --from=builder /build/client/mosquitto_pub /usr/bin/mosquitto_pub
COPY --from=builder /build/client/mosquitto_sub /usr/bin/mosquitto_sub
COPY --from=builder /build/client/mosquitto_rr /usr/bin/mosquitto_rr
COPY --from=builder /build/apps/mosquitto_passwd/mosquitto_passwd /usr/bin/mosquitto_passwd
COPY --from=builder /build/lib/libmosquitto.so.2.1.2 /usr/lib/libmosquitto.so.2.1.2
RUN ln -sf libmosquitto.so.2.1.2 /usr/lib/libmosquitto.so.1 \
    && ln -sf libmosquitto.so.1 /usr/lib/libmosquitto.so

USER mosquitto
EXPOSE 1883
ENTRYPOINT ["mosquitto"]
CMD ["-c", "/mosquitto/config/mosquitto.conf"]
