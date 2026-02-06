# syntax=docker/dockerfile:1.7-labs
FROM --platform=linux/amd64 nvidia/cuda:12.4.0-devel-ubuntu22.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive \
    TZ=Etc/UTC \
    RUSTUP_INIT_SKIP_PATH_CHECK=yes \
    CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    curl \
    git \
    gnupg \
    libclang-dev \
    libssl-dev \
    lsb-release \
    lsof \
    pkg-config \
    python3 \
    python3-pip \
    python3-venv && \
    rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile default --default-toolchain stable

ENV PATH=/root/.cargo/bin:/root/.risc0/bin:${PATH}

RUN curl -L https://risczero.com/install | INSTALLER_NO_PROMPT=1 bash && \
    rzup install && \
    rzup install risc0-groth16

WORKDIR /workspace

# Copy only the necessary files for building
COPY Cargo.toml rust-toolchain.toml ./
COPY host/ ./host/
COPY methods/ ./methods/
COPY api-server/ ./api-server/
COPY core/ ./core/

# Build the project
RUN cargo fetch --locked && \
    cargo build --locked --release --package api-server --features cuda

FROM --platform=linux/amd64 nvidia/cuda:12.4.0-runtime-ubuntu22.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive \
    TZ=Etc/UTC

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    lsb-release \
    lsof && \
    rm -rf /var/lib/apt/lists/*

RUN mkdir -p --mode=0755 /usr/share/keyrings && \
    curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | \
    tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null && \
    echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' > /etc/apt/sources.list.d/cloudflared.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends cloudflared && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /root/.risc0 /root/.risc0
ENV PATH=/root/.risc0/bin:${PATH}

WORKDIR /app

# Copy only the built binary from the builder stage
COPY --from=builder /workspace/target/release/api-server /usr/local/bin/api-server

EXPOSE 8080

ENTRYPOINT ["api-server"]
