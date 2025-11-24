FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static

WORKDIR /build

# Copy source code
COPY . .

# Build for release
RUN cargo build --release --bin yas-mcp

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /build/target/release/yas-mcp /app/yas-mcp

# Create necessary directories
RUN mkdir -p /app/config /app/examples

# Copy default config
COPY config.yaml /app/config/config.yaml.example

# Create non-root user
RUN addgroup -g 1000 app && \
    adduser -D -u 1000 -G app app && \
    chown -R app:app /app

USER app

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/app/yas-mcp", "--help"]

EXPOSE 3000

ENTRYPOINT ["/app/yas-mcp"]
CMD ["--mode", "http", "--host", "0.0.0.0", "--port", "3000"]