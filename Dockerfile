# Author: Passwd Word
# Multi-stage build for p2p-http-gateway.
# This image is self-contained: it includes the Rust gateway AND the
# ARM P2P engine sidecar (qemu-arm + libforcetv.so + bionic shim).

FROM rust:1.89-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY frontend ./frontend
# Single build: no dummy-main caching trick (it caused stale binaries
# when the Docker layer cache failed to invalidate).
RUN cargo build --release && \
    ls -lh /app/target/release/p2p-http-gateway && \
    test $(stat -c%s /app/target/release/p2p-http-gateway) -gt 1000000

FROM debian:bookworm-slim
# qemu-user-static: run ARM engine on x86_64 hosts.
# (On arm64 hosts the 32-bit ARM engine runs natively via kernel compat.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates qemu-user-static \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/p2p-http-gateway /app/
COPY frontend ./frontend
# Engine sidecar files (ARM): launcher, libp2p_patched.so, lib/
COPY engine /opt/p2p-engine
ENV BIND=0.0.0.0:8080
ENV ENGINE_ROOT=/opt/p2p-engine
EXPOSE 8080
CMD ["/app/p2p-http-gateway"]
