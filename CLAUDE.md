# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Yaatal Engine** is a sovereign Rust workspace (Loco / "Rust on Rails") providing reusable
backend infrastructure for African-first AI applications. **YOKK** and **BOBO** are the first
consumers; NJOOBA / DAARA follow.

**Prime directive — engine/app boundary:** everything in `crates/` must be **app-agnostic**.
App-specific code (UI, app-named flows) lives in `apps/` only. The one deliberate exception is the
BOBO commerce *bridge* controllers in `yaatal-api` (`bobo_checkout`, `bobo_orders`, `bobo_kyc`,
`merchant`), which are BOBO-shaped surfaces over app-agnostic primitives — keep new app coupling out
of the other crates.

## Build / lint / test (these are the CI gates — match them exactly)

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings   # warnings are errors in CI
cargo test --workspace -- --test-threads=1              # tests are NOT parallel-safe (shared DB state)
```

- **Single test:** `cargo test -p <crate> <name> -- --test-threads=1` (e.g. `cargo test -p yaatal-api feed -- --test-threads=1`).
- **ALSA on headless Linux:** the full workspace build pulls `yaatal-voice`, which links ALSA only
  when its `edge`/`legacy-cpal-whisper` feature is on. Default features are empty, so server builds
  need no audio libs. If a voice build does need ALSA, install `libasound2-dev`, or run
  `cargo test -p yaatal-voice --no-default-features`. See `KNOWN-ISSUES.md`.
- **Windows / OneDrive:** cargo is flaky under the OneDrive-synced working dir. Use the helper gates:
  `./scripts/run-rust-gates.ps1 -Mode all` (and `./scripts/setup-rust-test-env.ps1` to set a target
  dir outside OneDrive + import the MSVC toolchain).
- `cargo audit` runs in CI with two documented `--ignore`d advisories (see `.github/workflows/rust-ci.yml`
  + `KNOWN-ISSUES.md`); any *other* advisory fails CI.

## Run locally

```bash
docker compose -f docker-compose.dev.yml up -d   # Postgres (PostGIS 17) + PgBouncer
cp .env.example .env                             # set JWT_SECRET; SILICONFLOW_API_KEY unlocks AI tiers 2-3
cargo run -p migration -- up                     # apply migrations
cargo run -p yaatal-api                          # binds 0.0.0.0:5150 in dev
```

Health check: `GET /_health` (Loco default) or `GET /health` (custom). Authed routes are under
`/api/*` and expect `Authorization: Bearer <jwt>` from `POST /api/auth/login`.

## Configuration model (important, non-obvious)

- **No config is hardcoded in Rust.** Loco reads `config/<env>.yaml` and interpolates `${VAR}` /
  `{{ get_env(...) }}`. Env is selected by `LOCO_ENV` (`development` default, `production` in Docker).
- **Two config dirs exist:** workspace `config/` and `crates/yaatal-api/config/`. The `yaatal_api-cli`
  binary auto-selects the crate-local config when launched from the workspace root. Production uses
  `crates/yaatal-api/config/production.yaml`.
- **Required env to boot:** `JWT_SECRET` and `DATABASE_URL` (no defaults — a missing value crashes
  boot at config render). Optional keys (AI tiers, S3/R2, Wave payments, PostHog, OneSignal) are
  documented in `README.md` and `.env.example`.

## Architecture (the big picture — spans many files)

- **5-tier AI cascade router** (`crates/yaatal-core/src/ai/`): cheapest/on-device first → frontier
  cloud last. `router.rs` holds a per-tier `HashMap<&str, CircuitBreaker>` (5-fail / 5-min cool-off,
  `circuit_breaker.rs`), gated by `network.rs` (NetworkGate) and `rate_limit.rs` (RateLimiterPool).
  Tier providers: SiliconFlow (LFM2 / Qwen), OpenRouter/Anthropic (Claude, sensitive-capable), HF.
  With no API keys it falls back to Tier 1 and returns `AllTiersExhausted`.
- **Sovereignty type system** (`crates/yaatal-core/src/policy/` + `storage/`): `Sensitivity
  { Sovereign | Operational | Public }`, a sealed `SensitivityTag` trait, and `Tagged<T, S>` markers.
  `StorageDispatcher` routes data by tag; `R2Store` is **compile-time gated to `Public` only** (a
  doctest proves PII cannot reach the edge). Respect these tags when adding storage paths.
- **Loco app wiring** (`crates/yaatal-api/src/app.rs`): the `Hooks::routes` fn registers every
  controller's `routes()` (each controller owns its path + `.prefix()`); `after_routes` attaches the
  analytics dispatcher + LiveKit config as Axum `Extension`s. New endpoints = new controller module +
  one `.add_route(...)` line + `pub mod` in `controllers/mod.rs`.
- **Persistence:** sea-orm over Postgres. **The live migrator is the sea-orm `migration` crate at
  `crates/yaatal-api/migration/`** (registered in its `lib.rs`); `production.yaml` sets
  `auto_migrate: true` so it runs on boot. Root `migrations/001_initial.sql` is legacy/raw and not the
  source of truth.
- **Payments** (`crates/yaatal-payments/`): normalized `contract.rs` + `RailSelector` + idempotent
  `EventStore` + webhook router. Wave is the only real rail (HMAC-SHA256; OM/FM/Card/Crypto stub to
  `RailNotConfigured`). NOTE: the BOBO `bobo_checkout` controller writes payment intents via raw SQL
  and bypasses this crate — there are currently two payment paths; prefer consolidating onto
  `yaatal-payments` for new work.
- **Feed** (`crates/yaatal-feed/`): a staged pipeline `sources → filters → scorers → hydrators →
  selectors`; ingestion is deliberately separated from the ranking core.
- **Search** (`crates/yaatal-search/`): runnable `/search` + `/index/upsert` service over a BGE-M3
  Python sidecar + Qdrant, with versioned embedding profiles (`canonical-1024`, `edge-512/256/128`).
- **Voice** (`crates/yaatal-voice/`): WebSocket session server with a PersonaPlex-compatible mock
  backend; `build.rs` only invokes cmake/bindgen under the `speech-core-sys` feature (off by default;
  `third_party/speech-core` is a git submodule).

## Test-backend gotcha (read before writing controller tests)

Tests run against in-memory **SQLite**, production runs **Postgres**. Postgres-only DDL is gated on
the backend, and some endpoints (e.g. BOBO checkout) short-circuit with **503 "requires Postgres"**
when `ctx.db.get_database_backend() != Postgres`. Don't assume Postgres features in tests; check
`KNOWN-ISSUES.md` for the documented Postgres-vs-SQLite harness drift and the `#[ignore]`d
voice-routing recovery-state test.

## Deployment

- **Railway** service `Yaatal-Engine` builds with the **Dockerfile** (not railpack — `railway.json`'s
  builder is `DOCKERFILE`), deployed via `railway up` (no GitHub source). Postgres + Redis are
  provisioned in the project; Redis is not required to boot (workers run `BackgroundAsync` in-process).
- **The Rust builder image must be ≥ 1.85** — transitive deps (e.g. `clap 4.x`) require the
  `edition2024` Cargo feature. An older `rust:1.82` image fails at manifest parse.
- **`.dockerignore` / `.railwayignore` use gitignore semantics** for the upload: anchor root-only
  excludes with a leading `/` (`/data`, `/target`, …). A bare `data/` or `**/bin` will silently strip
  source modules like `crates/yaatal-api/src/data/` or `.../src/bin/` and break the build.

## Source-of-truth docs

`README.md` and `SPRINT-LOG.md` are current. `ARCHITECT-ENGINE.md` carries a continuity/mutation-log
protocol but its phase tracker and stack table are **explicitly stale** (still reference Turso/libSQL,
retired by the Postgres cutover) — trust README/SPRINT-LOG over it.
