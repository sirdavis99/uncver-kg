FROM rust:1.75-bookworm as builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && rustup default stable

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release && \
    strip /app/target/release/kg-core

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/kg-core /usr/local/bin/kg-core

RUN mkdir -p /data /config

ENV KG_DATA_DIR=/data
ENV KG_CONFIG_DIR=/config

ENTRYPOINT ["kg-core"]
