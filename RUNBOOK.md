# RUNBOOK — Yaatal Engine

Operational procedures for local development, first-time boot, payments testing,
and common failure modes.

> All commands assume `bash` and a Rust toolchain (`rustup show` ≥ 1.81).
> See [KNOWN-ISSUES.md](KNOWN-ISSUES.md) for platform-specific caveats.

---

## First-Time Boot

```bash
# 1. Start Postgres 17 + PgBouncer
docker compose -f docker-compose.dev.yml up -d

# 2. Wait for Postgres to accept connections (~5 s)
until docker compose -f docker-compose.dev.yml exec db pg_isready -U yaatal; do
  sleep 1
done

# 3. Copy and edit environment file
cp .env.example .env
# Minimum required edits:
#   JWT_SECRET=<random 32+ char string>
# Optional for AI tiers:
#   SILICONFLOW_API_KEY=sk-...
#   OPENROUTER_API_KEY=sk-or-...
# Optional for payments testing:
#   WAVE_API_KEY=...
#   WAVE_WEBHOOK_SECRET=...

# 4. Run database migrations
cargo run -p migration -- up

# 5. Start the API server
cargo run -p yaatal-api
# Server binds to 0.0.0.0:5150 in development mode
```

Verify the server is up:

```bash
curl -sf http://localhost:5150/_health && echo "OK"
```

---

## Restarting After a Code Change

```bash
# Rebuild and restart (Ctrl-C the running server first)
cargo build -p yaatal-api && cargo run -p yaatal-api
```

For a faster incremental cycle install `cargo-watch`:

```bash
cargo install cargo-watch
cargo watch -x "run -p yaatal-api"
```

---

## Wiping the Local Database

```bash
# Stops containers and removes the named Postgres volume
docker compose -f docker-compose.dev.yml down -v
# Re-run First-Time Boot from step 1 to recreate a fresh DB
```

---

## Exercising Payments End-to-End

The `yaatal-payments` crate (Lane 5a) is shipped — Wave adapter, HMAC webhook,
idempotent event log all ready. The HTTP wiring into `yaatal-api`
(`/api/payments/*`) is in progress via Lane 4-lite; the curl recipes below are
the target shape. Once the controller lands they will work as written.

### (a) Acquire a JWT

```bash
JWT=$(curl -sf -X POST http://localhost:5150/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "dev@example.com", "password": "dev-password"}' \
  | python3 -c "import sys, json; print(json.load(sys.stdin)['token'])")
echo "JWT: $JWT"
```

### (b) Initiate a payment

```bash
curl -sf -X POST http://localhost:5150/api/payments/initiate \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{
    "rail": "wave",
    "amount_xof": 5000,
    "currency": "XOF",
    "idempotency_key": "test-run-001",
    "instrument_hint": "wave_senegal"
  }'
```

Expected response shape:

```json
{
  "handle": { "provider_ref": "wv_...", "status": "Pending" }
}
```

### (c) Simulate a Wave webhook

Sign the payload body with HMAC-SHA256 using `WAVE_WEBHOOK_SECRET`, then POST to
the webhook intake endpoint:

```bash
BODY='{"event":"payment.completed","provider_ref":"wv_test_001","amount":5000}'
WAVE_WEBHOOK_SECRET="${WAVE_WEBHOOK_SECRET:-dev-webhook-secret}"

SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$WAVE_WEBHOOK_SECRET" | awk '{print $2}')

curl -sf -X POST http://localhost:5150/api/payments/webhook/wave \
  -H "Content-Type: application/json" \
  -H "X-Wave-Signature: $SIG" \
  -d "$BODY"
```

---

## Troubleshooting

### Postgres connection refused

```
Error: could not connect to server: Connection refused
```

**Checks:**
1. `docker compose -f docker-compose.dev.yml ps` — confirm the `db` service is
   `Up` (not `Exited`).
2. `docker compose -f docker-compose.dev.yml logs db | tail -20` — look for
   `database system is ready to accept connections`.
3. Confirm `DATABASE_URL` in `.env` matches the exposed port:
   `DATABASE_URL=postgres://yaatal:yaatal@localhost:5432/yaatal_dev`

### pgvector extension not found

```
ERROR: extension "vector" is not available
```

**Cause.** The Postgres image does not include pgvector.

**Fix.** Confirm the `docker-compose.dev.yml` `db` service uses the PostGIS image:

```yaml
image: postgis/postgis:17-3.5
```

Then re-run `docker compose -f docker-compose.dev.yml down -v && docker compose -f docker-compose.dev.yml up -d`
to recreate the container. After that, re-run the extensions migration:

```bash
cargo run --manifest-path crates/yaatal-api/migration/Cargo.toml -- up
```

The extensions migration (`m20260601_000000_extensions.rs`) runs
`CREATE EXTENSION IF NOT EXISTS vector;` before the schema migrations.

### ALSA build failure on headless Linux

```
error: could not find native library `alsa`
```

See [KNOWN-ISSUES.md](KNOWN-ISSUES.md).

Quick fix:

```bash
sudo apt-get install -y libasound2-dev
```

Or exclude `yaatal-voice` and build the rest:

```bash
cargo build --workspace --exclude yaatal-voice
```

### `cargo run -p migration` — package not found

**Cause.** Workspace member resolution lost the migration path. The crate is at
`crates/yaatal-api/migration/` and is mounted via `yaatal-api/Cargo.toml`'s
`migration = { path = "migration" }` dep (not a top-level workspace member).

**Fix.** Run from the explicit manifest path:

```bash
cargo run --manifest-path crates/yaatal-api/migration/Cargo.toml -- up
```

---

## `.env.example` Audit — Stale Entry Report

The following variables appear in `.env.example` but are **not yet read by any
Rust source file** in `crates/` on this branch. Most flow through the Loco-rs
YAML config loader (`config/*.yaml`) via `${VAR}` interpolation.

| Variable | Status |
|---|---|
| `TURSO_DATABASE_URL` | **RETIRED in Lane 1.** May still exist in `.env.example` — safe to delete. |
| `TURSO_AUTH_TOKEN` | **RETIRED in Lane 1.** May still exist in `.env.example` — safe to delete. |
| `JWT_SECRET` | Config-only (Loco-rs YAML loader interpolation). Still required. |
| `SILICONFLOW_API_KEY` | Config-only. The `AiRouter` receives its key via `AiConfig` struct populated by the config loader. |
| `HUGGINGFACE_API_KEY` | Config-only. Same as above. |
| `OPENROUTER_API_KEY` | Config-only. Same as above. |
| `ANTHROPIC_API_KEY` | Config-only. Same as above. |
| `S3_BUCKET` | Config-only (`config/production.yaml`). Dev uses disk storage. |
| `AWS_*` | Config-only. No Rust read. |
| `POSTHOG_API_KEY` | Config-only. No Rust read. |
| `ONESIGNAL_*` | Config-only. No Rust read. |

**Summary.** Most `.env.example` entries are consumed by the YAML config loader.
The two `TURSO_*` entries should be deleted in a follow-up. The `WAVE_*` set
(`WAVE_API_BASE`, `WAVE_API_KEY`, `WAVE_WEBHOOK_SECRET`, `WAVE_MERCHANT_ID`)
is documented in the README Configuration Matrix and read directly by
`PaymentsService::from_env` once Lane 4-lite's payments controller lands.
