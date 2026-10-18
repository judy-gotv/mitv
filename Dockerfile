# Author: Passwd Word
# Multi-stage build for p2p-http-gateway.
# NOTE: the engine sidecar (qemu-arm + ARM .so) is NOT in this image;
# mount it at /opt/p2p-engine and set ENGINE_ROOT accordingly.
# The engine itself is ARM; on x86_64 hosts the sidecar needs qemu-arm-static
# either in the image or on the host. See README.md.

FROM rust:1.89-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
# Cache deps first (dummy main to compile deps)
RUN mkdir src && echo 'fn main(){}' > src/main.rs && \
    cargo build --release && rm -rf src
COPY src ./src
COPY frontend ./frontend
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/p2p-http-gateway /app/
COPY frontend ./frontend
ENV BIND=0.0.0.0:8080
ENV ENGINE_ROOT=/opt/p2p-engine
EXPOSE 8080
CMD ["/app/p2p-http-gateway"]
