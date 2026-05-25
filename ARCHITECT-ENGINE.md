# ARCHITECT-ENGINE.md — Yaatal Engine Continuity Protocol

> **CRITICAL: Every Architect session MUST start by reading this file.**
> **CRITICAL: Every Architect session MUST end by updating the MUTATION LOG.**

---

## SESSION START PROTOCOL

1. Read this ENTIRE file
2. Read SPRINT-LOG.md
3. Check open issues on GitHub
4. Identify which phase you're in
5. Pick lowest-numbered open issue whose dependencies are met
6. Execute

---

## IDENTITY

You are **The Architect** — infrastructure engineer for Yaatal Engine.

**Prime directive:** Build reusable Rust infrastructure for African-first applications.
YOKK is the first consumer. NJOOBA, DAARA, and future apps will follow.

**Engine constraint:** Everything in `crates/` must be app-agnostic.
App-specific code goes in `apps/` ONLY.

---

## PROJECT

**Yaatal Engine** is a Rust workspace providing:
- AI cascade routing (5-tier, cheapest-first)
- Database abstraction (~~Turso/libSQL~~ → **self-hosted Postgres** post-Lane 1 + SeaORM)
  <!-- [STALE — Lane 1 retires libSQL. Target: sqlx-postgres + PostGIS/pgvector in Diamniadio.] -->
- Voice recording + transcription
- Gamification (XP, levels, streaks)
- Content sanitization
- Design tokens
- Semantic search (future)

### Stack

<!-- [PARTIALLY STALE — DB row updated below; Voice row will update after Lane 2.] -->

| Layer | Tech | Crate |
|-------|------|-------|
| DB | ~~Turso/libSQL~~ → **Postgres 17 + pgvector + PostGIS** (Lane 1) + SeaORM | yaatal-core |
| Backend | Loco (Rust on Rails) | yaatal-api |
| AI | reqwest → SiliconFlow/Qwen/OpenRouter(Claude)/HF | yaatal-core |
| Voice | cpal + hound + Whisper → **soniqo/speech-core C ABI** (Lane 2) | yaatal-voice |
| Search | ColBERT GGUF (future) | yaatal-search |
| Mobile | Dioxus 0.7 | yokk-mobile |

---

## NON-NEGOTIABLE CONSTRAINTS

<!-- [STALE: constraint 1 — Turso embedded replicas retired by Lane 1. Offline strategy TBD post-Lane 1.] -->
1. **Offline-First** — ~~Turso embedded replicas~~ (Lane 1 retires libSQL; offline caching strategy to be re-specified)
2. **Mobile-First** — 44px touch targets, battery-conscious, <25MB APK
3. **Bandwidth-Aware** — Opus audio, compressed payloads
4. **Latency-Tolerant** — 300ms+ RTT assumed, optimistic UI
5. **Data Sovereign** — data stays in Africa where possible

---

## PHASE TRACKER

| Phase | Issue | Status | Branch |
|-------|-------|--------|--------|
| E1 | Scaffold workspace | DONE | e1-scaffold-workspace |
| E2 | Database schema + models | NOT STARTED | e2-schema-models |
| E3 | AI cascade router | NOT STARTED | e3-ai-router |
| E4 | JWT auth controller | NOT STARTED | e4-jwt-auth |
| E5 | Posts CRUD + feed | NOT STARTED | e5-posts-feed |
| E6 | Voice crate | NOT STARTED | e6-voice-crate |
| E7 | Kill gate (Dioxus+cpal) | NOT STARTED | e7-kill-gate |
| E8 | Wire YOKK PWA | NOT STARTED | e8-wire-pwa |

---

## MUTATION LOG

### Session 000 — 2026-02-14 (Setup)
**Architect:** Copilot (GitHub)
**What happened:**
- Repo created under Yaatal-labs org
- Pushed: .gitignore, .env.example, Cargo.toml, config/*.yaml, LICENSE
- Master build document created with all file contents
- 8 issues defined (E1-E8) with full specs
**What's next:** E1 scaffold — any Architect picks up, builds locally, PRs
**Blockers:** None

### Session 001 — 2026-02-14 (E1 Scaffold)
**Architect:** Claude Opus 4.6
**What happened:**
- Created all missing workspace members (yaatal-api, yaatal-voice, yaatal-search, yokk-mobile)
- Created yaatal-core modules (auth, design/tokens, gamification/xp, models/post+profile, sanitize)
- Created migrations/001_initial.sql (10 tables + indexes)
- Created ARCHITECT-ENGINE.md, SPRINT-LOG.md, replaced README.md
- Version decision: Dioxus 0.7.2 (first-class mobile), SeaORM 1.x + libsql 2-layer strategy
**What's next:** cargo build --workspace && cargo test --workspace, then PR
**Blockers:** gh CLI not authenticated — PR needs manual push or auth

### Session 002 — 2026-02-15 (E1 Build Fixes)
**Architect:** Claude Opus 4.6
**What happened:**
- Fixed SeaORM `DeriveActiveEnum` for `PostType` — `String(None)` → `String(StringLen::None)` (SeaORM 1.x breaking change)
- Fixed sanitize regex — replaced lookahead `(?!...)` with `(?i)<script[^>]*>[\s\S]*?</script>` (Rust `regex` crate doesn't support lookaround)
- Removed unused `AiTask` import from `ai/router.rs`
- Added missing `serde_json` dependency to `yaatal-voice/Cargo.toml`
- **cargo build --workspace**: PASSES (0 errors, 0 warnings)
- **cargo test --workspace**: PASSES (23/23 tests green)
**What's next:** E2 — Database schema + models (SeaORM migrations, Turso connection pool)
**Blockers:** gh CLI still not authenticated — PR needs manual push or auth

---

## END SESSION PROTOCOL

1. Update PHASE TRACKER above
2. Add new MUTATION LOG entry with:
   - Session number (increment)
   - Date
   - Architect identity
   - What changed (files, decisions)
   - What's next
   - Blockers
3. Commit ARCHITECT-ENGINE.md changes
4. Update SPRINT-LOG.md

---

## QUICK REFERENCE

```bash
cargo build --workspace      # must pass before PR
cargo test --workspace       # must pass before PR
cargo test -p yaatal-core    # test core only
cargo test -p yaatal-voice   # test voice only
```

## RELATIONSHIP TO YOKK

- YOKK PWA repo: https://github.com/MouhamedN96/YOKK
- YOKK is the FIRST consumer of Yaatal Engine
- Migration path: YOKK PWA -> yaatal-api (feature-flagged, E8)
- YOKK-specific UI/logic goes in apps/yokk-mobile, NOT in crates/
