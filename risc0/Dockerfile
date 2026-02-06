# --- Builder stage ---
FROM nvidia/cuda:12.8.0-devel-ubuntu24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    curl build-essential pkg-config libssl-dev clang libclang-dev protobuf-compiler python3 git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile default
ENV PATH="/root/.cargo/bin:${PATH}"

# Install RISC Zero toolchain via rzup
RUN curl -L https://risczero.com/install | INSTALLER_NO_PROMPT=1 bash
ENV PATH="/root/.risc0/bin:${PATH}"
RUN rzup install && \
    rzup install risc0-groth16

# Copy source
WORKDIR /build
COPY . .

# Build the api-server with CUDA support
RUN RUSTFLAGS="-C target-cpu=native" cargo build --release --package api-server --features cuda

# --- Runtime stage ---
FROM nvidia/cuda:12.8.0-runtime-ubuntu24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Install cloudflared
RUN curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 \
    -o /usr/local/bin/cloudflared && \
    chmod +x /usr/local/bin/cloudflared

# Copy binary and RISC Zero data
COPY --from=builder /build/target/release/api-server /usr/local/bin/api-server
COPY --from=builder /root/.risc0 /root/.risc0

EXPOSE 8080

ENTRYPOINT ["api-server"]
