# Yaatal Engine

**AI-native infrastructure for African-first applications. Built in Rust.**

> Owned by YAATAL LABS LLC

---

## What is this?

Yaatal Engine is a reusable Rust workspace that provides shared infrastructure
for African-first applications. The first consumer is
[YOKK](https://github.com/MouhamedN96/YOKK) — a community platform for
African tech builders.

## Architecture

```
crates/yaatal-core     — AI router, models, gamification, design tokens
crates/yaatal-api      — Loco HTTP backend
crates/yaatal-voice    — cpal recording + Whisper transcription
crates/yaatal-search   — ColBERT semantic search (future)
apps/yokk-mobile       — YOKK Dioxus mobile app
```

## Crates

| Crate | Purpose | Status |
|-------|---------|--------|
| `yaatal-core` | Shared types, AI cascade, XP system, models | In Progress |
| `yaatal-api` | Loco REST API | Scaffold |
| `yaatal-voice` | Audio recording + transcription | Scaffold |
| `yaatal-search` | Semantic search | Planned |
| `yokk-mobile` | YOKK Dioxus frontend | Planned |

## Getting Started

```bash
# Clone
git clone https://github.com/Yaatal-labs/Yaatal-Engine.git
cd Yaatal-Engine

# Build
cargo build --workspace

# Test
cargo test --workspace

# Environment
cp .env.example .env
# Fill in your API keys
```

## Railway

`Yaatal-Engine` deploys the `yaatal-api` binary on Railway. For a fresh service or a drifted one, use:

```powershell
.\scripts\railway-bootstrap.ps1
.\scripts\railway-bootstrap.ps1 -Apply
```

The bootstrap script verifies the sibling Postgres service, ensures the required runtime variables exist, and triggers a single redeploy. Full notes live in [`docs/deployment/railway.md`](docs/deployment/railway.md).

## Configuration

This repo has two config trees:

- `crates/yaatal-api/config/` — Loco runtime config for the deployed API
- `config/` — engine-level workspace config from the initial scaffold

Railway boots `yaatal-api` from `crates/yaatal-api/config/`, not the root `config/` folder.

## License

MIT OR Apache-2.0
