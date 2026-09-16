# Multi-stage. `FROM scratch` runtime (Option 2): a static musl binary with
# webpki-roots CA bundled in and hickory pure-Rust DNS, so no libc, no
# /etc/ssl/certs, no /etc/passwd, no shell, no resolv.conf. TLS (Keycloak JWKS +
# Postgres via rustls) and DNS resolve without any OS-provided config.
#
# NOTE: migrations are NEVER run here (D1/D11) — they run in a dedicated step.

# --- Build stage (static musl, arch-dynamic) ---
FROM rust:1.95-alpine AS builder
WORKDIR /app
# Alpine is musl-native; its `*-alpine-linux-musl-gcc` links fully static (no
# glibc, no .so). The toolchain is arch-specific, so we derive the target and
# the C compiler from `uname -m` — builds clean on arm64 hosts AND amd64 CI,
# no hardcoded arch.
RUN apk add --no-cache musl-dev build-base \
    && rustup target add "$(uname -m)-unknown-linux-musl"
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
# Set the arch-specific CC/CXX/linker for cc-rs's `ring` C build, then compile
# for the matching musl target. Everything is static: no .so to load at runtime.
RUN TARGET="$(uname -m)-unknown-linux-musl" \
    && CCBIN="$(uname -m)-alpine-linux-musl-gcc" \
    && cargo build --release \
        --target "$TARGET" \
        --config "target.$TARGET.linker=\"$CCBIN\"" \
        --config "target.$TARGET.rustflags=[\"-C\",\"target-feature=+crt-static\"]"

# --- Runtime: scratch (no libc, no shell, no CA dir, no /etc/passwd) ---
FROM scratch AS runtime
COPY --from=builder /app/target/*-unknown-linux-musl/release/settings-service /settings-service
USER 65532:65532
EXPOSE 8080
# No HEALTHCHECK: on scratch probes go via k8s `httpGet` to /healthz (liveness)
# and /readyz (readiness), not an exec'd curl/shell inside the container.
ENTRYPOINT ["/settings-service"]
