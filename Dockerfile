# Multi-stage build for Rusty Claw
# Produces a minimal image from scratch

# --- Build stage ---
FROM rust:1.85-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

RUN cargo build --release --bin rusty-claw \
    && strip target/release/rusty-claw

# --- Runtime stage ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/rusty-claw /usr/local/bin/rusty-claw

# Default config and workspace directories
RUN mkdir -p /data/config /data/workspace

ENV RUSTY_CLAW_CONFIG=/data/config/config.json
ENV RUSTY_CLAW_WORKSPACE=/data/workspace

EXPOSE 18789

ENTRYPOINT ["rusty-claw"]
CMD ["gateway", "--config", "/data/config/config.json", "--ui"]
