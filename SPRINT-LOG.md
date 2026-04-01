# SPRINT LOG — Yaatal Engine

## Day 0 — 2026-02-14 (Setup)
**Goal:** Scaffold workspace, create issues, establish SSOT
**Completed:**
- [x] Repo created under Yaatal-labs org
- [x] Root files pushed (.gitignore, .env.example, Cargo.toml, configs)
- [x] Master build document created
- [x] Issues E1-E8 defined

## Day 1 — 2026-02-14 (E1 SCAFFOLD)
**Goal:** E1 scaffold + start E7 kill gate
**Status:** IN PROGRESS
**Completed:**
- [x] All workspace crates created (yaatal-core, yaatal-api, yaatal-voice, yaatal-search, yokk-mobile)
- [x] yaatal-core modules: ai, auth, design, gamification, models, sanitize
- [x] migrations/001_initial.sql — 10 tables + indexes
- [x] ARCHITECT-ENGINE.md + SPRINT-LOG.md
- [x] Version decisions: Dioxus 0.7.2, SeaORM 1.x + libsql 2-layer
**Pending:**
- [x] cargo build --workspace passes
- [x] cargo test --workspace passes (23/23)
- [ ] PR opened and merged
**Decision:** E1 scaffold complete. Build and tests green. PR blocked on gh CLI auth.

## Day 2 — 2026-02-15 (E1 Build Fixes)
**Goal:** Get E1 builds and tests passing
**Status:** DONE
**Completed:**
- [x] Fixed SeaORM DeriveActiveEnum (String(None) → String(StringLen::None))
- [x] Fixed sanitize regex (lookahead → simple pattern)
- [x] Added missing serde_json dep to yaatal-voice
- [x] Removed unused AiTask import
- [x] cargo build --workspace passes (0 errors)
- [x] cargo test --workspace passes (23/23 green)
**Pending:**
- [ ] PR opened and merged (gh CLI auth blocker)

## Day 3 — 2026-04-01 (Railway Deploy Hardening)
**Goal:** Make Railway deployment wiring repeatable
**Status:** DONE
**Completed:**
- [x] Hardened `yaatal-api` startup to auto-select `crates/yaatal-api/config` when run from the workspace root
- [x] Added Railway bootstrap script to verify Postgres, wire `DATABASE_URL` and `JWT_SECRET`, and trigger a single redeploy
- [x] Improved the Windows Rust gate helper to add Git `cp.exe` and import MSVC build tools automatically when available
- [x] Documented the dual-config layout and Railway deploy flow in `README.md` and `docs/deployment/railway.md`
- [x] Confirmed Railway service `Yaatal-Engine` is healthy after bootstrap
**Pending:**
- [ ] Merge deploy hardening branch
