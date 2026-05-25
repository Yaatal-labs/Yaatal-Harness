# Yaatal Engine

Sovereign Rust backend for African-first AI applications. Powers YOKK (community platform) and upcoming NJOOBA / DAARA apps. Provides a 5-tier AI cascade router, gamification engine, voice pipeline, and payment primitives — all in a single `cargo build`.

> Owned by YAATAL LABS LLC · MIT OR Apache-2.0

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/Yaatal-labs/Yaatal-Engine.git
cd Yaatal-Engine

# 2. Start Postgres + PgBouncer
docker compose -f docker-compose.dev.yml up -d

# 3. Configure environment
cp .env.example .env
$EDITOR .env   # minimum: set JWT_SECRET; add SILICONFLOW_KEY for AI tiers 2-3;
               # add WAVE_* vars only when testing payments

# 4. Run migrations
cargo run -p migration -- up

# 5. Start the API server (binds to 0.0.0.0:5150 in development)
cargo run -p yaatal-api
```

### Probe endpoints

Once the server is running:

```bash
# Unauthenticated health check
curl http://localhost:5150/_health

# Authenticated example — swap <jwt> for a token from /api/auth/login
curl -H "Authorization: Bearer <jwt>" http://localhost:5150/api/posts
```

---

## Workspace Layout

| Crate / App | Description |
|---|---|
| `crates/yaatal-core` | Shared types, models, AI router, and business logic for Yaatal Engine |
| `crates/yaatal-api` | Loco-based HTTP backend for Yaatal Engine |
| `crates/yaatal-voice` | Voice recording, encoding, and transcription for Yaatal Engine |
| `crates/yaatal-search` | ColBERT search and retrieval for Yaatal Engine (future) |
| `apps/yokk-mobile` | YOKK mobile app — Dioxus frontend |

All crates in `crates/` are **app-agnostic**. YOKK-specific code lives in `apps/` only.

---

## Configuration Matrix

Environment variables are read by the Loco config loader (`config/*.yaml`).
None are hardcoded in Rust source; all use `${VAR}` interpolation.

| Variable | Used by | Required? | Default (dev) | What it unlocks |
|---|---|---|---|---|
| `JWT_SECRET` | `yaatal-api` auth | **Required** | `yaatal-dev-secret-change-in-production` | Signs / verifies all JWTs |
| `DATABASE_URL` | `yaatal-api` | **Required** | `postgres://yaatal:yaatal_dev@localhost:5432/yaatal_dev` | Postgres connection for sea-orm |
| `SILICONFLOW_API_KEY` | `yaatal-core` AI router | Optional | — | Enables Tier 2 (LFM2-1.2B) and Tier 3 (Qwen2.5-72B) |
| `HUGGINGFACE_API_KEY` | `yaatal-core` AI router | Optional | — | Enables Tier 5 fallback (Mistral-7B via HF Inference) |
| `OPENROUTER_API_KEY` | `yaatal-core` AI router | Optional | — | Enables Tier 4 (Claude Sonnet 4 via OpenRouter) — sensitive-capable |
| `ANTHROPIC_API_KEY` | `yaatal-core` AI router | Optional | — | Direct Anthropic Tier 4 path |
| `S3_BUCKET` | `yaatal-api` storage | Optional | `./storage` (disk) | S3-compatible object store (production) |
| `AWS_ACCESS_KEY_ID` | `yaatal-api` storage | Optional | — | S3 / R2 credential |
| `AWS_SECRET_ACCESS_KEY` | `yaatal-api` storage | Optional | — | S3 / R2 credential |
| `AWS_REGION` | `yaatal-api` storage | Optional | — | S3 region (use `af-south-1` for Africa) |
| `POSTHOG_API_KEY` | `yaatal-api` analytics | Optional | — | Product analytics via PostHog |
| `ONESIGNAL_APP_ID` | `yaatal-api` push | Optional | — | Push notification delivery |
| `ONESIGNAL_REST_API_KEY` | `yaatal-api` push | Optional | — | Push notification auth |
| `WAVE_API_BASE` | `yaatal-payments` | Optional | `https://api.wave.com/v1` | Wave Business API endpoint (`[Unverified]` placeholder per TASK.md §7) |
| `WAVE_API_KEY` | `yaatal-payments` | Optional | — | Wave mobile-money adapter bearer token |
| `WAVE_WEBHOOK_SECRET` | `yaatal-payments` | Optional | — | HMAC-SHA256 webhook signature verification |
| `WAVE_MERCHANT_ID` | `yaatal-payments` | Optional | — | NJOOBA's Wave merchant identifier |

> **AI cascade behaviour.** With no API keys set, the router falls back to Tier 1
> (on-device — not yet wired) and returns `AllTiersExhausted`. For local development
> set at minimum `SILICONFLOW_API_KEY` to exercise Tiers 2–3.

---

## Lanes Shipped

| Lane | Title | Status |
|---|---|---|
| Lane 0 | **yokk-engine merge** — AI evolution (TierConfig, NetworkGate, RateLimiterPool, OpenRouter Tier 4 Claude Sonnet 4, yaatal-feed pipeline, voice WS session at /api/voice/session, n8n webhooks, CI) | ✅ Shipped |
| Lane 1 | **Postgres cutover** — libSQL retired, `docker-compose.dev.yml` with PostGIS 17 + PgBouncer, sea-orm → sqlx-postgres, `m20260601_000000_extensions` enables vector / pg_cron / postgis | ✅ Shipped |
| Lane 2 | **speech-core FFI scaffold** — `soniqo/speech-core` C ABI submodule at `third_party/`, bindgen wrapper, feature-gated cmake build, safe Rust `Session` facade | ✅ Shipped |
| Lane 3 | **Circuit breaker + router wiring** — 5-fail / 5-min cool-off per tier, per-tier `HashMap<&str, CircuitBreaker>` in `AiRouter`, structured tracing events | ✅ Shipped |
| Lane 5a | **`yaatal-payments` crate** — internal payment wrapper per Notion TASK.md, T1–T8 complete: contract + selector + idempotent event log + webhook router + Wave adapter (HMAC-SHA256, `[Unverified]` endpoints) + OM/FM/Card/Crypto stubs returning `RailNotConfigured` + 39 tests | ✅ Shipped |
| Lane 6 | **Sensitivity types + storage dispatcher** — `Sensitivity { Sovereign / Operational / Public }`, sealed `SensitivityTag` trait, `Tagged<T, S>` marker type, `StorageDispatcher` trait, `R2Store` impl gated to `Public` only (compile-fail doctest proves it) | ✅ Shipped |
| Lane 4 | Bo-Plex tool contract (four canonical tools) | ⏳ Deferred — pending Gemma 4 adapter training |
| Lane 5b | BOBO commerce schema (orders / ledger / escrow / kyc / payment_intents) | 🟡 In progress (Sonnet subagent) |
| Lane 4-lite | Payments HTTP wiring into `yaatal-api` | 🟡 In progress (Sonnet subagent) |

---

## Testing

```bash
# Full workspace (requires ALSA dev libs on Linux — see KNOWN-ISSUES.md)
cargo test --workspace --exclude yaatal-voice

# Voice crate without ALSA / audio device
cargo test -p yaatal-voice --no-default-features
```

**ALSA gotcha.** On headless Linux, `cargo test --workspace` will fail unless
`libasound2-dev` (Debian/Ubuntu) or `alsa-lib-devel` (Fedora/RHEL) is installed.
See [KNOWN-ISSUES.md](KNOWN-ISSUES.md#ki-001--alsa-sys-link-failure-on-headless-linux).

---

## Known Issues

See [KNOWN-ISSUES.md](KNOWN-ISSUES.md) for a full list of open issues, workarounds,
and stale-documentation annotations.

---

## Architecture Deep-Dive

The engine uses a 5-tier cascade router (cheapest/on-device first, frontier cloud last),
a sovereign-first data model (self-hosted Postgres in Diamniadio + Cloudflare R2 for
public-only edge), and a sensitivity tagging system that prevents PII from leaving the
sovereign tier. See [ARCHITECT-ENGINE.md](ARCHITECT-ENGINE.md) for the full continuity
protocol and mutation log.
