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

Those surfaces now exist on dedicated branches:

| Branch | Commit | State |
|--------|--------|-------|
| `codex/search-service` | `02b10a8` | runnable phase-one search service; green |
| `codex/voice-service` | `b587e0b` | runnable PersonaPlex-compatible local mock/service; green |
| `codex/engine-orchestrator` | `4c9876b` | Engine wired to both service contracts; green on targeted `yaatal-api` gates |

Important: those states are **not yet merged** into `codex/deploy-candidate` or `main`.

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
| `crates/yaatal-api` | Deployed backend and the main Bo-Plex orchestration surface | Canonical branch: transcribe-only; engine lane: session-wired |
| `crates/yaatal-feed` | Generic feed/discovery ranking engine | Integrated |
| `crates/yaatal-voice` | Audio utilities, mock voice service, and batch transcription fallback | Service lane green |
| `crates/yaatal-search` | Search service surface plus retrieval contracts/helpers | Service lane green |
| `apps/yokk-mobile` | Thin app/runtime probe; not the current engineering driver | Stub |

## Current direction

- **Engine is the orchestrator.** The app should stay thin.
- **PersonaPlex stays external.** Use a local mock first, RunPod later.
- **Search stays behind one HTTP contract.** The Engine calls `/search`; BGE-M3 and Qdrant stay behind that service.
- **Voice and search are independently runnable/testable on their service branches.** Keep them in the monorepo and preserve those service contracts during merge-back.
- **Redis and SigLIP2 are deferred.** First make the vocal loop work.
- **Legacy voice/search descriptions in older session logs are historical, not current target architecture.**

The older internal `Harness × Runtime` framing is only partially current now:

- still valid: stable contract over swappable internals
- needs reinterpretation: one standalone binary and HTTP-only voice
- obsolete: ColBERT / FocalCodec / LFM2-centered implementation assumptions

The detailed setup and implementation split live in [docs/architecture/boplex-setup.md](docs/architecture/boplex-setup.md).

## Next session focus

The next lowest-hanging task is to make the Engine locally testable end-to-end without changing the outer architecture again:

1. merge or temporarily stack `codex/search-service`, `codex/voice-service`, and `codex/engine-orchestrator`
2. start all three services locally
3. run one scripted `/api/voice/session` smoke flow against the mock voice service and real `/search` service
4. keep the current service contracts unchanged

That gets the Engine testable now while preserving the longer-term `Harness × Runtime` principle: stable boundary outside, swappable internals later.

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
- the first full local loop still needs to be run with all three services started together

The current Railway setup is documented in [docs/deployment/railway.md](docs/deployment/railway.md).

## Canonical docs

- [ARCHITECT-ENGINE.md](ARCHITECT-ENGINE.md) — project protocol, current stage, active execution lanes
- [SPRINT-LOG.md](SPRINT-LOG.md) — session history and current sprint-level status
- [docs/architecture/boplex-setup.md](docs/architecture/boplex-setup.md) — Bo-Plex implementation approach and worktree split
- [docs/deployment/railway.md](docs/deployment/railway.md) — current backend deployment path

## License

MIT OR Apache-2.0
