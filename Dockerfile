# OmniCore Discord bot — multi-stage build (static-ish Linux binary)
# Build:  docker build -t omnicore .
# Run:    docker run --env-file .env -v omnicore-data:/data omnicore

FROM rust:1.85-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev musl-tools \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml ./
# Copy sources before resolving deps so offline builds still work after first fetch
COPY src ./src
COPY assets ./assets

# Prefer lockfile when present
COPY Cargo.lock* ./

# No browser needed: wosoracle.com data is fetched anonymously
RUN cargo build --release --target x86_64-unknown-linux-musl --no-default-features \
    || (cargo generate-lockfile && cargo build --release --target x86_64-unknown-linux-musl --no-default-features)

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 -m omnicore
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/omnicore /usr/local/bin/omnicore
COPY assets /app/assets
RUN mkdir -p /data && chown -R omnicore:omnicore /data /app
USER omnicore
ENV DATABASE_PATH=/data/bot.sqlite
ENV RUST_LOG=info
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/omnicore"]
