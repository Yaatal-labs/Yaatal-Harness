FROM rust:1.90-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p yaatal-api --bin yaatal_api-cli

# ── Runtime ───────────────────────────────────────────────────────────────────
# MUST match the builder's Debian release (rust:1.90-slim is Trixie). A bookworm
# runtime ships GLIBC 2.36 and cannot exec a Trixie-built binary (GLIBC 2.38+).
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/yaatal_api-cli /app/yaatal_api-cli
COPY --from=builder /app/crates/yaatal-api/config /app/config

ENV LOCO_ENV=production
# Pin an ABSOLUTE config folder. main.rs::bootstrap_runtime_env() otherwise
# auto-sets LOCO_CONFIG_FOLDER to the relative source path
# `crates/yaatal-api/config`, which resolves to an empty/missing folder at
# runtime (cwd-dependent) and crash-loops Loco with "no configuration file
# found". Setting it explicitly bypasses that auto-detect.
ENV LOCO_CONFIG_FOLDER=/app/config
EXPOSE 8080

CMD ["/app/yaatal_api-cli", "start", "--environment", "production"]
