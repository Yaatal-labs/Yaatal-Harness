# Yaatal Engine

**Voice/session orchestration engine for grounded African-first applications. Built in Rust.**

> Owned by YAATAL LABS LLC

## What this repo is now

Yaatal Engine is a Rust workspace centered on one responsibility: **the Engine brokers authenticated app sessions to external services**.

The current near-term focus is **Bo-Plex**:

- client streams audio to the Engine over WebSocket
- Engine brokers that session to PersonaPlex
- Engine watches the upstream text stream
- Engine calls one real `/search` service over HTTP when grounding is needed
- Engine injects grounded context back into the live voice session

This is an R&D engine project, not a finished SaaS product. The goal is to make the orchestration loop real and testable first.

To get usability fast, the Bo-Plex build is now split into **three runnable surfaces** inside the monorepo:

- a voice service surface
- a search service surface
- the Engine orchestrator surface

## Current architecture

```text
Client UI (Flutter / thin probe)
  ↕ WebSocket + JSON envelopes
Yaatal Engine (Railway-hosted yaatal-api)
  ↕ WebSocket
PersonaPlex (local mock first, RunPod later)

Yaatal Engine
  ↕ HTTP
/search service
  ↕ internal retrieval stack
BGE-M3 + Qdrant
```

## Workspace surfaces

| Surface | Current role | Status |
|---------|--------------|--------|
| `crates/yaatal-core` | Shared domain models, AI/router primitives, DB helpers, gamification, sanitization | Real, broad |
| `crates/yaatal-api` | Deployed backend and the main Bo-Plex orchestration surface | Active focus |
| `crates/yaatal-feed` | Generic feed/discovery ranking engine | Integrated |
| `crates/yaatal-voice` | Audio utilities and batch transcription fallback; next home of thin PersonaPlex transport | In transition |
| `crates/yaatal-search` | Search evaluation and HTTP-client-side retrieval utilities, not the live session orchestrator | Experimental |
| `apps/yokk-mobile` | Thin app/runtime probe; not the current engineering driver | Stub |

## Current direction

- **Engine is the orchestrator.** The app should stay thin.
- **PersonaPlex stays external.** Use a local mock first, RunPod later.
- **Search stays behind one HTTP contract.** The Engine calls `/search`; BGE-M3 and Qdrant stay behind that service.
- **Voice and search should each become independently runnable/testable.** Keep them in the monorepo, but stop treating them as only internal helpers.
- **Redis and SigLIP2 are deferred.** First make the vocal loop work.
- **Legacy voice/search descriptions in older session logs are historical, not current target architecture.**

The detailed setup and implementation split live in [docs/architecture/boplex-setup.md](docs/architecture/boplex-setup.md).

## Getting started

```bash
git clone https://github.com/Yaatal-labs/Yaatal-Engine.git
cd Yaatal-Engine

cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
```

Copy the environment template and fill in the current runtime values:

```bash
cp .env.example .env
```

For Windows dev shells, use the bootstrap helpers in `scripts/` so Cargo sees `cp.exe` and the MSVC toolchain.

## Deployment shape

- Railway hosts `yaatal-api`
- Railway/Postgres is the current deployed database path
- PersonaPlex is an external service boundary
- `/search` is an external service boundary

The current Railway setup is documented in [docs/deployment/railway.md](docs/deployment/railway.md).

## Canonical docs

- [ARCHITECT-ENGINE.md](ARCHITECT-ENGINE.md) — project protocol, current stage, active execution lanes
- [SPRINT-LOG.md](SPRINT-LOG.md) — session history and current sprint-level status
- [docs/architecture/boplex-setup.md](docs/architecture/boplex-setup.md) — Bo-Plex implementation approach and worktree split
- [docs/deployment/railway.md](docs/deployment/railway.md) — current backend deployment path

## License

MIT OR Apache-2.0
