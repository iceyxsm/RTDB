# RTDB - Production-Grade Smart Vector Database
# Multi-stage build for optimized image size

# Stage 1: Builder
FROM rust:1.75-bookworm AS builder

WORKDIR /app

# Install protobuf compiler for gRPC support
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto/ ./proto/

# Copy source code
COPY src/ ./src/

# Build release binary
RUN cargo build --release --features grpc

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN groupadd -r rtdb && useradd -r -g rtdb rtdb

# Copy binary from builder
COPY --from=builder /app/target/release/rtdb /usr/local/bin/rtdb

# Create data directory
RUN mkdir -p /data && chown rtdb:rtdb /data

# Switch to non-root user
USER rtdb

# Expose ports
# 6333 - REST API (Qdrant compatible)
# 6334 - gRPC API (Qdrant compatible)
# 9090 - Prometheus metrics
EXPOSE 6333 6334 9090

# Volume for persistent data
VOLUME ["/data"]

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD rtdb status || exit 1

# Default command
ENTRYPOINT ["rtdb"]
CMD ["start"]
