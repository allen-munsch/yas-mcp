# ── Build stage ──
# Uses BuildKit cache mount for fast incremental builds:
#   docker build -t yas-mcp .
#   DOCKER_BUILDKIT=1 docker build -t yas-mcp .  (if BuildKit not default)

FROM rust:1.91-alpine AS builder

RUN apk add --no-cache \
    musl-dev pkgconfig openssl-dev openssl-libs-static protobuf-dev

WORKDIR /build
COPY . .

# BuildKit cache mount avoids recompiling unchanged crates
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --bin yas-mcp && \
    cp target/release/yas-mcp /tmp/yas-mcp

# ── Runtime stage ──
FROM alpine:3.19

RUN apk add --no-cache ca-certificates libgcc

WORKDIR /app

COPY --from=builder /tmp/yas-mcp /app/yas-mcp
COPY examples/ /app/examples/
RUN mkdir -p /app/config

COPY config.yaml /app/default.yaml

RUN addgroup -g 1000 app && \
    adduser -D -u 1000 -G app app && \
    chown -R app:app /app

USER app

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/app/yas-mcp", "--help"]

EXPOSE 3000

ENTRYPOINT ["/app/yas-mcp", "--config", "/app/default.yaml"]
CMD []
