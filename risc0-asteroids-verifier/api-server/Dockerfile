# syntax=docker/dockerfile:1.7-labs

ARG CUDA_DEVEL_IMAGE=vastai/base-image:cuda-12.9.1-auto
ARG CUDA_RUNTIME_IMAGE=vastai/base-image:cuda-12.9.1-auto

FROM --platform=linux/amd64 ${CUDA_DEVEL_IMAGE} AS builder

ENV DEBIAN_FRONTEND=noninteractive \
    TZ=Etc/UTC \
    RUSTUP_INIT_SKIP_PATH_CHECK=yes \
    CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup \
    CARGO_TERM_COLOR=always

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    curl \
    git \
    libclang-dev \
    libssl-dev \
    protobuf-compiler \
    pkg-config && \
    rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile default --default-toolchain stable

ENV PATH=/root/.cargo/bin:/root/.risc0/bin:${PATH}

# Install the RISC Zero toolchain required by methods/build.rs guest compilation.
RUN curl -L https://risczero.com/install | INSTALLER_NO_PROMPT=1 bash && \
    rzup install && \
    rzup install risc0-groth16 || true

WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY asteroids-core ./asteroids-core
COPY methods ./methods
COPY host ./host
COPY api-server ./api-server

ARG ENABLE_CUDA=0
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/workspace/target \
    if [ "${ENABLE_CUDA}" = "1" ]; then \
        cargo build --locked --release -p api-server --features cuda; \
    else \
        cargo build --locked --release -p api-server; \
    fi

FROM --platform=linux/amd64 ${CUDA_RUNTIME_IMAGE} AS runtime

ENV DEBIAN_FRONTEND=noninteractive \
    TZ=Etc/UTC \
    API_BIND_ADDR=0.0.0.0:8080 \
    RUST_LOG=info \
    RISC0_DEV_MODE=0

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /root/.risc0 /root/.risc0
ENV PATH=/root/.risc0/bin:${PATH}

WORKDIR /app
COPY --from=builder /workspace/target/release/api-server /usr/local/bin/api-server

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=40s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1

ENTRYPOINT ["api-server"]
