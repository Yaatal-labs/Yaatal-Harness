FROM rust:1.90-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p yaatal-api --bin yaatal_api-cli

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/yaatal_api-cli /app/yaatal_api-cli
COPY --from=builder /app/crates/yaatal-api/config /app/config

ENV LOCO_ENV=production
EXPOSE 8080

CMD ["/app/yaatal_api-cli", "start", "--environment", "production"]
