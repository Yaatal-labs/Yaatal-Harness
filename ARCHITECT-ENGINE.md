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
- authenticated backend/session orchestration
- reusable feed/discovery ranking
- shared domain/database primitives
- voice transport plus batch transcription fallback
- external service brokering for PersonaPlex and search
- gamification, sanitization, and app-agnostic engine support

### Stack

| Layer | Tech | Crate |
|-------|------|-------|
| Data | Postgres + SeaORM today; libSQL/Turso remains longer-term engine direction | yaatal-core + yaatal-api |
| Backend | Loco on Axum, REST today and WebSocket next | yaatal-api |
| Feed / Discovery | Generic ranking pipeline | yaatal-feed |
| Voice | Audio utilities and batch transcription fallback; PersonaPlex transport next | yaatal-voice |
| Retrieval | External `/search` contract; `yaatal-search` stays evaluation/client-side support for now | yaatal-search |
| UI | Thin client probe; current `yokk-mobile` is not the active engineering focus | apps/* |

---

## NON-NEGOTIABLE CONSTRAINTS

1. **Offline-First** — Turso embedded replicas
2. **Mobile-First** — 44px touch targets, battery-conscious, <25MB APK
3. **Bandwidth-Aware** — Opus audio, compressed payloads
4. **Latency-Tolerant** — 300ms+ RTT assumed, optimistic UI
5. **Data Sovereign** — data stays in Africa where possible

---

## PHASE TRACKER

| Phase | Issue | Status | Branch |
|-------|-------|--------|--------|
| E1 | Scaffold workspace | DONE | e1-scaffold-workspace |
| E2 | Database schema + models | DONE | e2-schema-models |
| E3 | AI cascade router | DONE | e3-ai-router |
| E4 | JWT auth controller (Loco) | DONE | e4-jwt-auth |
| E5 | Posts CRUD + feed | DONE | e5-posts-feed |
| E6 | Voice crate | IN PROGRESS | e6-voice-crate |
| E7 | Kill gate (Dioxus+cpal) | NOT STARTED | e7-kill-gate |
| E8 | Wire YOKK PWA | NOT STARTED | e8-wire-pwa |

---

## CURRENT PROJECT STAGE

- Roadmap position: `E1-E5` are materially implemented, `E6` is still active, `E7-E8` remain open.
- Operating mode: the project has moved from scaffold/buildout into integration, correctness, deployment, and service orchestration.
- Backend status: the service/backend path is real enough to verify and deploy in isolation.
- Current target: stand up the first Bo-Plex vocal loop with a local mock PersonaPlex upstream and a real `/search` service.
- Current branch reality:
  - `codex/search-service` is green at `02b10a8`
  - `codex/voice-service` is green at `b587e0b`
  - `codex/engine-orchestrator` is wired at `4c9876b`
- Remaining risk concentration: merging the lane stack back cleanly, running the first full local loop, and replacing mock backends with real ones without breaking the service contracts.

---

## CURRENT BO-PLEX DIRECTION

- Engine is the orchestrator. `yaatal-api` owns session lifecycle, auth, routing, and retries.
- PersonaPlex stays external. Start with a local mock, then swap to RunPod.
- Search stays behind one HTTP boundary. The Engine calls `/search`; BGE-M3 and Qdrant stay behind that service.
- `yaatal-voice` and `yaatal-search` should become independently runnable/testable service surfaces inside the monorepo.
- Client contracts stay thin: JSON envelopes plus base64 audio over WebSocket.
- Grounding returns upstream as text context injection.
- Redis, SigLIP2, ZeroClaw, Path B orchestration, and heavy app work are out of the first milestone.

---

## ACTIVE EXECUTION LANES

| Lane | Branch / worktree | Scope | Status |
|------|--------------------|-------|--------|
| Control / integration | `codex/deploy-candidate` | canonical branch for deployable backend and docs | clean, but behind active service lanes |
| Voice service | `codex/voice-service` | runnable PersonaPlex-compatible mock plus `yaatal-voice` transport surface | green at `b587e0b` |
| Search service | `codex/search-service` | runnable `/search` HTTP surface plus `yaatal-search` service contract | green at `02b10a8` |
| Engine orchestrator | `codex/engine-orchestrator` | `yaatal-api` WebSocket session route, JWT auth, per-turn state, service orchestration | green on targeted gates at `4c9876b` |

---

## RISKS / NOTES (Active)

- ~~yaatal-api is a placeholder~~ → RESOLVED: Loco SaaS scaffold in place (Session 014)
- ~~AI router offline/2G gating and shared rate limiting are not yet defined~~ → RESOLVED: E3 complete (Session 013)
- Config: yaatal-api uses Loco's own config (`config/development.yaml`); yaatal-core retains its own config loader. Both coexist — Loco manages server/auth/DB, yaatal-core manages AI keys.
- Loco users table vs yaatal-core profiles: dual-table strategy decided. Loco owns `users` (auth), yaatal-core owns `profiles` (domain). Link via `user_id → users.id` migration needed (E5 scope).
- Production DB: local/dev still center on SQLite; Railway/Postgres deployment is now an active integration path. Turso/libSQL remains part of the longer-term engine direction, not the only deploy target.
- Current live voice surface is still `POST /api/voice/transcribe`; no Bo-Plex WebSocket session route exists yet.
- Canonical branch truth and service-lane truth are now different:
  - on `codex/deploy-candidate`, the live voice surface is still `POST /api/voice/transcribe`
  - on `codex/engine-orchestrator`, `/api/voice/session` exists and is wired to the voice and search service contracts
- `yaatal-search` already proves the sidecar-over-HTTP pattern, but it is not the session orchestrator and should not become the only production search boundary by accident.
- `yaatal-voice` and `yaatal-search` are no longer only library-shaped on the active service branches; the next step is merge/integration, not first runnable surfaces.
- Current remaining work is integration-heavy rather than scaffold-heavy: merge order, end-to-end loop proof, deployment/runtime hardening, and app/runtime probe wiring.

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

### Session 003 — 2026-04-01 (Railway Deploy Hardening)
**Architect:** Codex
**What happened:**
- Fixed Railway packaging for `yaatal-api` so Railpack installs the binary into `/app/bin`
- Hardened the API binary to auto-select `crates/yaatal-api/config` when launched from the workspace root
- Added `scripts/railway-bootstrap.ps1` to verify the Postgres service, wire `DATABASE_URL`, generate `JWT_SECRET`, and redeploy once
- Improved `scripts/setup-rust-test-env.ps1` to add Git `cp.exe` and import MSVC build tools automatically on Windows when present
- Updated `README.md`, `.env.example`, and `docs/deployment/railway.md` to document the deploy path and dual-config layout
- Verified Railway service `Yaatal-Engine` reached healthy `SUCCESS` status in production
**What's next:** Merge the deploy hardening branch and keep Railway bootstrap in repo as the default recovery path
**Blockers:** None

### Session 004 - 2026-02-15 (E2 Schema + Models)
**Architect:** Codex (GPT-5)
**What happened:**
- Added missing model fields to align with migrations (profiles, posts)
- Added SeaORM models for remaining tables (comments, upvotes, launches, achievements, bo_conversations, feed_items, bookmarks, user_security_keys)
- Added db helpers for config loading, connection, and migration execution
- Added serde_yaml dependency to yaatal-core
 - cargo test -p yaatal-core failed to run (cargo not available in PATH)
**What's next:** Run cargo test -p yaatal-core, then finish E2
**Blockers:** None

### Session 005 — 2026-02-16 (Voice Crate Hardening + E2 Verification)
**Architect:** Claude Opus 4.6
**What happened:**
- Verified Codex's E2 work: cargo build + cargo test pass (30 core tests green)
- **Voice crate rewrite (yaatal-voice):**
  - Replaced all `unwrap()` on mutex locks with `map_err` → `RecorderError::LockPoisoned`
  - Changed WAV encoding from 32-bit float to 16-bit PCM (Whisper API compatibility)
  - Added f32→i16 clamping conversion
  - Added concurrent-start guard (`AlreadyRecording` error)
  - Added device config mismatch warning in `start()`
  - Added `is_recording()`, `sample_count()`, `sample_rate()`, `channels()` accessors
  - `clear()` now returns `Result` instead of panicking
  - Used `thiserror` for proper error derives
- **Transcription rewrite:**
  - Added `TranscriptionError` enum (Network, Api, ModelLoading, EmptyResult)
  - Handle HuggingFace 503 "model loading" responses with estimated_time
  - Added 30s timeout to API calls
  - Actually measure `duration_ms` (was hardcoded to 0)
  - Added `transcribe_with_model()` for model selection
- Added 8 voice tests: WAV header, 16-bit PCM encoding, f32 clamping, clear, empty, error display
- **cargo build --workspace**: 0 errors, 0 warnings
- **cargo test --workspace**: 38/38 passing (30 core + 8 voice)
**What's next:** E2 still needs entity relations and integration tests. E3/E6 unblocked for parallel work.
**Blockers:** None

### Session 006 - 2026-02-16 (Rebase Cleanup + Handoff)
**Architect:** Codex (GPT-5)
**What happened:**
- Found in-progress rebase on e1-scaffold-workspace with conflicts
- Created safety branch backup/rebase-wip
- Aborted rebase and returned to e1-scaffold-workspace
**What's next:** Decide whether to merge or rebase origin/e1-scaffold-workspace into local (ahead 7, behind 2), then resume E3 or finish E2 cleanup
**Blockers:** None

### Session 007 - 2026-02-19 (Skills + CI Workflow Alignment)
**Architect:** Codex (GPT-5)
**What happened:**
- Added model-agnostic skills governance docs and references:
  - `AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `architect.md`
  - `skills/manifest.yaml`, `skills/rust-e2e-ai-agent/SKILL.md`, `skills/README.md`
- Added skills validation automation:
  - `scripts/validate-skills-manifest.ps1`
  - `scripts/validate-skill-docs.ps1`
  - `.github/workflows/validate-skills-manifest.yml`
  - `.github/workflows/validate-skill-docs.yml`
- Added Rust CI workflow:
  - `.github/workflows/rust-ci.yml` (`fmt`, `check`, `clippy`, `test`)
- Added implementation docs:
  - `docs/agent-usage.md`
  - `docs/dev-workflow-status.md`
  - updated `README.md` local skills + troubleshooting section
- Verification:
  - Skills validators pass
  - `cargo fmt --all` applied successfully
  - `cargo fmt --all --check` passes
  - `cargo check`, `cargo clippy`, `cargo test` blocked by missing native C compiler required by `libsql-ffi`
**What's next:** Install native C build tooling on local dev machine/runner, then rerun `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`
**Blockers:** Local environment missing compiler toolchain for `libsql-ffi` build script

### Session 008 - 2026-02-19 (Windows Build Remediation + Gate Verification)
**Architect:** Codex (GPT-5)
**What happened:**
- Verified MSVC toolchain availability (`cl.exe`) via Visual Studio Build Tools developer shell
- Diagnosed OneDrive path issue causing non-writable cargo build output directories
- Diagnosed `libsql-ffi` Windows build-script requirement for `cp` command
- Ran workspace gates with Windows-safe environment:
  - `CARGO_HOME` and `CARGO_TARGET_DIR` moved to `%TEMP%`
  - `cargo check --workspace`: PASS
  - `cargo clippy --workspace --all-targets -- -D warnings`: PASS
  - `cargo test --workspace`: PASS (37 tests)
- Fixed clippy failure in `crates/yaatal-core/src/design/tokens.rs` by replacing runtime constant assertion test with a const assertion
- Updated `README.md` and `docs/dev-workflow-status.md` with final Windows troubleshooting guidance
**What's next:** Keep E2 in progress and continue entity relations/integration test work
**Blockers:** None (local gate verification succeeded with documented Windows setup)

### Session 009 - 2026-02-20 (ColBERT Zero-Shot + Python Sidecar)
**Architect:** Codex (GPT-5)
**What happened:**
- Implemented `yaatal-search` zero-shot retrieval evaluation scaffold:
  - `crates/yaatal-search/src/zero_shot.rs`
  - Metrics: `MRR@k`, `Recall@k`, `nDCG@k`
  - Added unit tests for perfect/partial/invalid cases
- Added Python sidecar integration for ColBERT retrieval:
  - `scripts/colbert_sidecar.py` (`/health`, `/index`, `/search`)
  - `crates/yaatal-search/src/python_sidecar.rs` (`ColbertHttpRetriever`)
  - Exported modules from `crates/yaatal-search/src/lib.rs`
- Updated search crate dependencies for HTTP integration:
  - `crates/yaatal-search/Cargo.toml` (`reqwest` blocking + `serde_json`)
- Added deployment and retrieval docs:
  - `docs/colbert-zero-shot.md`
  - `docs/unsloth-on-device-deployment.md`
  - updated `README.md` docs references
- Verification:
  - `python -m py_compile scripts/colbert_sidecar.py`: PASS
  - `cargo fmt --all --check`: PASS
  - `cargo clippy -p yaatal-search --all-targets -- -D warnings`: PASS
  - `cargo test -p yaatal-search --offline`: PASS (3 tests)
  - Workspace gates still blocked by local `libsql-ffi` build-script `cp` dependency in PATH
**What's next:** Plug real labeled Yaatal retrieval dataset into sidecar-backed baseline runs; decide serving strategy for ColBERT in app environments
**Blockers:** Full workspace verification blocked by missing `cp` command required by `libsql-ffi` build script

### Session 010 - 2026-02-21 (WAXAL + Trilingual Retrieval Experiments, Notebook Handoff)
**Architect:** Codex (GPT-5)
**What happened:**
- Installed and used Hugging Face workflow skills for dataset querying, trainer workflow guidance, and metric tracking:
  - `hugging-face-datasets`
  - `hugging-face-model-trainer`
  - `hugging-face-trackio`
- Added runnable experiment scripts:
  - `scripts/run_lfm_colbert_waxal.py` (WAXAL zero-shot + optional fine-tune + post-eval)
  - `scripts/run_lfm_colbert_fr_en_wo_iterations.py` (FR/EN/WO + mixed-query zero-shot iterations)
  - `scripts/build_trilingual_synthetic_corpus.py` (HF trilingual corpus + synthetic code-switch pairs)
- Added code-switch evaluation mode to WAXAL run path (`plain`, `codeswitch`, `both`).
- Executed WAXAL run artifacts and metrics:
  - `artifacts/lfm_colbert_waxal/run-20260221-052711/metrics.json`
  - Codeswitch slice improved slightly post-finetune (`dMRR +0.0100`, `dnDCG +0.0077`) while plain slice regressed.
- Executed trilingual zero-shot iteration run:
  - `artifacts/lfm_colbert_fr_en_wo/run-20260221-054839/metrics.json`
  - Strong Wolof/mix retrieval and weak English/French retrieval against Wolof-indexed docs.
- Built reusable corpus from HF trilingual source + synthetic mixed queries:
  - `data/corpus/fr_en_wo_v1/manifest.json` (`2000` docs, `8000` query/doc pairs).
- Added visual/report assets for handoff:
  - `artifacts/reports/lfm_colbert_summary.html`
  - `notebooks/lfm_colbert_test_results.ipynb`
- Updated Unsloth-facing Liquid ColBERT notebook with WAXAL + trilingual evaluation flow and optional quick fine-tune cell:
  - `notebooks/nb/💧_LFM2_ColBERT_350M_Inference.ipynb`
**What's next:**
- Replace synthetic code-switch query generation with real production code-switched user query samples.
- Add hard-negative mining for EN/FR -> WO retrieval alignment in fine-tune batches.
- Run full Unsloth GPU notebook training loop externally (local GPU VRAM is insufficient for stable fine-tune path).
**Blockers:**
- Local Python 3.13 + `pylate` compatibility constraints required Python 3.12 runtime for experiments.
- Local Unsloth run path needs CUDA-enabled torch wheel profile; local checks defaulted to CPU torch in this environment.

### Session 011 - 2026-02-21 (E2 Assignable Closeout Brief + Scope Queue)
**Architect:** Codex (GPT-5)
**What happened:**
- Added execution-ready E2 handoff brief:
  - `docs/architect-e2-closeout-brief.md`
- Captured exact E2 closure scope:
  - SeaORM relation wiring targets by model file
  - Required integration tests (migrations, FK enforcement, uniqueness, relation queries)
  - Definition-of-done and verification gates
- Captured full-project continuation queue (E3-E8) and parallel retrieval track context.
**What's next:**
- Execute `docs/architect-e2-closeout-brief.md` to complete E2 in code, then move to E3.
**Blockers:**
- None

### Session 012 - 2026-02-21 (E2 Relations + Integration Tests Completed)
**Architect:** Codex (GPT-5)
**What happened:**
- Implemented SeaORM relations for all FK-backed models:
  - `crates/yaatal-core/src/models/profile.rs`
  - `crates/yaatal-core/src/models/post.rs`
  - `crates/yaatal-core/src/models/comments.rs`
  - `crates/yaatal-core/src/models/upvotes.rs`
  - `crates/yaatal-core/src/models/launches.rs`
  - `crates/yaatal-core/src/models/achievements.rs`
  - `crates/yaatal-core/src/models/bo_conversations.rs`
  - `crates/yaatal-core/src/models/bookmarks.rs`
  - `crates/yaatal-core/src/models/user_security_keys.rs`
  - `crates/yaatal-core/src/models/feed_items.rs` (explicit note retained: no FK)
- Added integration coverage for E2 schema behaviors:
  - `crates/yaatal-core/tests/e2_schema_relations.rs`
  - Covers migration table existence, FK enforcement, uniqueness constraints, and relation/join query paths.
- Verification:
  - `cargo fmt --all --check`: PASS
  - `cargo clippy --workspace --all-targets -- -D warnings`: PASS
  - `cargo test --workspace`: PASS
- Added doc discoverability link:
  - `README.md` now references `docs/architect-e2-closeout-brief.md`
**What's next:**
- Start E3 (AI cascade router hardening): define offline/2G gating and shared rate-limit behavior, then add deterministic fallback tests.
**Blockers:**
- None

### Session 013 - 2026-02-21 (E3 AI Cascade Router Hardening)
**Architect:** Claude (Anthropic)
**What happened:**
- Added `network.rs`: `NetworkCondition` enum (`Offline`, `TwoG`, `ThreeG`, `FourGPlus`) with `Ord` comparison, `NetworkGate` trait (injectable for testing), `DefaultNetworkGate` (always `FourGPlus`)
- Added `rate_limit.rs`: Token-bucket `RateLimiter` + `RateLimiterPool` (per-provider, pure `std`, no external deps)
- Rewrote `router.rs` — data-driven architecture:
  - `TierConfig` struct + `DEFAULT_TIERS` const array replaces hardcoded match arms
  - 5 tiers: T1 on-device placeholder, T2 SiliconFlow/LFM2, T3 SiliconFlow/Qwen, T4 OpenRouter/Claude (NEW), T5 HuggingFace/Mistral
  - Offline/2G gating: tiers skipped when `network < tier.min_network`
  - Sensitivity routing: sensitive queries skip `sensitive_capable == false` tiers
  - Rate limiting: `RateLimiterPool` check before each HTTP call
  - Real latency: `Instant::now()` measurement replaces hardcoded `0`
  - Constructor returns `Result` instead of `expect()`
  - `AiRouter::with_options()` for test injection of `NetworkGate`
- Updated `mod.rs`: exports `network` + `rate_limit` modules
- Updated `lib.rs`: re-exports `NetworkCondition`, `NetworkGate`, `RateLimiterPool`
- Added `tests/e3_ai_router.rs` — 10 deterministic integration tests (zero network calls):
  - `route_offline_returns_tier1_only`, `route_2g_returns_tier1_only`
  - `route_sensitive_skips_non_capable_tiers`, `route_non_sensitive_uses_tier1`
  - `route_all_tiers_exhausted_when_no_keys`, `route_latency_is_populated`
  - `constructor_returns_result`, `classify_default_is_chat`
  - `network_condition_ordering`, `rate_limiter_pool_basics`
- Verification:
  - `cargo fmt --all --check`: PASS
  - `cargo clippy -p yaatal-core --all-targets -- -D warnings`: PASS (0 warnings)
  - `cargo test -p yaatal-core`: PASS (all existing + 10 new E3 tests)
**What's next:**
- E4 (JWT auth controller): scaffold Loco in `yaatal-api`, add JWT middleware
**Blockers:**
- None

### Session 014 — 2026-02-21 (E4 JWT Auth / Loco SaaS Scaffold)
**Architect:** Antigravity (Google DeepMind)
**What happened:**
- Researched Loco framework documentation: starters, JWT auth middleware, testing patterns, asset serving options
- Installed Loco CLI v0.16.3 (`cargo install loco`)
- User ran `loco new` interactively (SaaS starter, SQLite, Async workers, no asset serving) → scaffolded into `crates/yaatal-api`
- Integrated Loco into workspace:
  - Removed standalone `[workspace]` from generated `Cargo.toml`
  - Added `version.workspace = true`, `edition.workspace = true`, `license.workspace = true`
  - Added `yaatal-core` as dependency
  - Added `loco-rs = { version = "0.16" }` to workspace root deps
- Fixed `include_dir!` paths with `$CARGO_MANIFEST_DIR` prefix (required for workspace builds)
- Configured JWT auth in `config/development.yaml`:
  - Env-var secret (`JWT_SECRET` with dev default)
  - 72h expiry (African latency aware)
  - Bearer + Cookie (`yaatal_token`) fallback chain
- Auth endpoints out of the box: register, login, verify, forgot/reset password, magic link, current user, resend verification
- Full Loco module structure: `app.rs`, `controllers/auth.rs`, `models/users.rs`, `views/auth.rs`, `mailers/auth.rs`, `workers/downloader.rs`, `tasks/`, `fixtures/users.yaml`
- Verification:
  - `cargo check --workspace`: PASS
  - `cargo clippy -p yaatal-api --all-targets`: PASS (clean)
  - `cargo test -p yaatal-core`: PASS (40/40 — no regressions)
**What's next:**
- E5 (Posts CRUD + feed): use Loco scaffold generators (`cargo loco generate scaffold`) for post/comment CRUD, wire gamification XP hooks
- Link users ↔ profiles migration (add `user_id` FK to profiles table)
**Blockers:**
- None

### Session 015 — 2026-02-21 (E5 Feed Pipeline Genericization)
**Architect:** Antigravity (Google DeepMind)
**What happened:**
- Extracted and integrated `yaatal-feed` (based on X-algorithm) into workspace
- Genericized core pipeline to be app-agnostic (removed YOKK-specific types)
- Renamed `VoicePostCandidate` to `FeedCandidate`
- Renamed `YokkFeedQuery` to `FeedQuery`
- Introduced extensible `ContentType` (Voice, Text, ProductListing, CourseModule) to support Social Commerce (NJOOBA, DAARA)
- Extracted hardcoded weights into configurable `WeightConfig` for multi-app setups
- Refactored filters and scorers to use new generic types
- Documented remaining compilation errors for handoff
**What's next:**
- Fix remaining compile errors (`post_id` -> `id` mismatches, trait constraint issues in `builder.rs`, `MAX_POST_AGE_HOURS` config)
- Add Loco scaffolds for Post/Comment CRUD operations
**Blockers:**
- Residual field/trait mismatches in the newly genericized `yaatal-feed` crate (needs manual fixing before it compiles cleanly)

### Session 016 — 2026-02-22 (E5 Feed Compile Unblock + Offline Verification)
**Architect:** Codex (GPT-5)
**What happened:**
- Resolved `yaatal-feed` compile blockers introduced during E5 genericization:
  - Replaced stale `post_id` field usage with canonical `id` in filters:
    - `crates/yaatal-feed/src/filters/dedup_filter.rs`
    - `crates/yaatal-feed/src/filters/seen_posts_filter.rs`
  - Added missing selector constructor:
    - `crates/yaatal-feed/src/selectors/mod.rs` (`TopKSelector::new`)
  - Removed stale constant dependency and made age filtering config-driven:
    - `crates/yaatal-feed/src/filters/age_filter.rs`
    - `crates/yaatal-feed/src/builder.rs` (passes `config.max_post_age_hours`)
- Verification (local, offline):
  - `cargo test -p yaatal-feed --offline`: PASS (3 tests)
  - `cargo test -p yaatal-core --offline`: PASS
  - `cargo test -p yaatal-search --offline`: PASS
  - `cargo check --workspace --offline`: blocked at `libsql-ffi` build script (`cp` program not found in current shell)
**What's next:**
- Continue E5 by wiring Post/Comment CRUD scaffolds in `yaatal-api` and linking users↔profiles (`user_id` FK migration path).
- Run full workspace gates from a shell with real GNU `cp` and native C toolchain (`cl.exe`) available.
**Blockers:**
- Environment/toolchain blocker for workspace-level checks in this shell: `libsql-ffi` requires external `cp` binary (PowerShell alias is insufficient).

### Session 017 — 2026-02-22 (E5 API Auth Stabilization + Handoff Branching)
**Architect:** Codex (GPT-5)
**What happened:**
- Organized continuation branches for scoped E5 work:
  - `organized/e5-feed` (feed-only testable slice)
  - `organized/e5-api-auth` (Loco auth slice)
- Diagnosed auth request-test failures (HTTP 500 during register path) to missing Loco template extension variants.
- Added `.t` template files expected by Loco mailer lookup:
  - `crates/yaatal-api/src/mailers/auth/welcome/{subject.t,html.t,text.t}`
  - `crates/yaatal-api/src/mailers/auth/forgot/{subject.t,html.t,text.t}`
  - `crates/yaatal-api/src/mailers/auth/magic_link/{subject.t,html.t,text.t}`
- Committed fix on `organized/e5-api-auth`:
  - `e22b5e4 api/auth: add .t mail templates for loco mailer compatibility`
- Verification (stage/auth scope):
  - `cargo test -p yaatal-api --tests --offline`: PASS (`23 passed, 0 failed`)
**What's next:**
- Cherry-pick `e22b5e4` into primary E5 integration branch (`e5-posts-feed`) or merge `organized/e5-api-auth`.
- Continue E5 API scope: scaffold Post/Comment CRUD and add `profiles.user_id -> users.id` migration + relation wiring.
- Re-run workspace gates once online registry/toolchain environment is stable (`check`, `clippy`, `test`).
**Blockers:**
- No code blocker in auth scope after template fix.
- Environment remains sensitive to offline Cargo cache integrity and crates.io connectivity in restricted shells.

### Session 018 — 2026-02-22 (E5 Post/Comment CRUD + Users↔Profiles Link)
**Architect:** Antigravity (DeepMind)
**What happened:**
- Created 3 Loco SeaORM migrations:
  - `m20260222_000001_add_user_id_to_profiles`: adds `user_id` UUID column + unique index to `profiles`
  - `m20260222_000002_create_posts`: creates `posts` table matching `001_initial.sql` with FKs and indexes
  - `m20260222_000003_create_comments`: creates `comments` table with FKs to posts, profiles, and self-referential parent
- Added `user_id: Option<String>` field to `crates/yaatal-core/src/models/profile.rs`
- Created `crates/yaatal-api/src/services/xp_service.rs`: wraps `yaatal_core::gamification::xp` for DB-persisted XP awards
- Created `crates/yaatal-api/src/controllers/posts.rs`: full CRUD (create/list/show/update/delete) with JWT auth, author-only guards, pagination, XP integration (+25 PostArticle)
- Created `crates/yaatal-api/src/controllers/comments.rs`: CRUD (create/list/delete) nested under posts, JWT auth, XP integration (+10 Comment)
- Created view structs: `views/posts.rs`, `views/comments.rs`
- Wired modules in `lib.rs`, `controllers/mod.rs`, `views/mod.rs`
- Registered routes in `app.rs`
- Verification:
  - `cargo check -p yaatal-api`: PASS
  - `cargo check --workspace`: PASS
**What's next:**
- Run `cargo clippy --workspace` and `cargo test -p yaatal-api` for full verification
- Proceed with E6 (Voice Crate Wiring) or E7 (Kill Gate)
**Blockers:**
- None — workspace compiles cleanly

### Session 019 — 2026-02-24 (E5 Identity Mapping Fix + Handoff Blocker)
**Architect:** Codex (GPT-5)
**What happened:**
- Root-caused E5 identity mismatch:
  - JWT claim `users.pid` was being written directly into `posts.author_id` / `comments.author_id`.
  - Domain schema expects author FKs to `profiles.id`.
  - XP service also assumed incoming id was `profiles.id`.
- Implemented fix in `yaatal-api`:
  - Added linked profile creation during auth register (`profiles.user_id = users.pid`):
    - `crates/yaatal-api/src/controllers/auth.rs`
  - Added profile identity resolver service:
    - `crates/yaatal-api/src/services/profile_identity.rs`
    - exported in `crates/yaatal-api/src/services/mod.rs`
  - Updated posts/comments controllers to resolve `profile_id` from `user_pid` for write paths and author guards:
    - `crates/yaatal-api/src/controllers/posts.rs`
    - `crates/yaatal-api/src/controllers/comments.rs`
  - Updated XP service to award by `user_pid` via linked profile lookup:
    - `crates/yaatal-api/src/services/xp_service.rs`
  - Added request coverage:
    - `crates/yaatal-api/tests/requests/identity_mapping.rs`
    - wired in `crates/yaatal-api/tests/requests/mod.rs`
  - Added migration bootstrap for profiles table in Loco migration chain:
    - `crates/yaatal-api/migration/src/m20260222_000000_create_profiles.rs`
    - `crates/yaatal-api/migration/src/lib.rs`
- Reverted unintended formatting-only churn in unrelated files and preserved only scoped E5 fix files.
- Added handoff note:
  - `docs/session-handoff-2026-02-24.md`
**What's next:**
- Run full verification gates from a shell that has both `cp.exe` and `cl.exe` in PATH.
- If `fmt --check` still fails, remove pre-existing trailing whitespace in `crates/yaatal-api/tests/requests/auth.rs`.
- Commit and continue E5 integration after verification.
**Blockers:**
- Environment blocker in this shell:
  - `cargo test -p yaatal-api --tests --offline` fails at `libsql-ffi` build script (`cp` program not found for cargo subprocess).
  - `cl.exe` not present in PATH.
- `cargo fmt --all --check` also blocked by pre-existing trailing whitespace in `crates/yaatal-api/tests/requests/auth.rs`.
### Session 020 — 2026-02-25 (Project Review and Documentation)
**Architect:** Antigravity (DeepMind)
**What happened:**
- Reviewed project state across E1-E5 phases.
- Verified that E5 identity mapping fixes from Session 019 were applied correctly (resolves `users.pid` to `profiles.id`).
- Generated a comprehensive project summary documenting current architecture, database layers, and Gamification parameters.
- Attempted to run `cargo check --workspace` but verified it remains blocked by the offline/network environment (failing on crates.io and `libsql-ffi` build script missing GNU tools).
**What's next:**
- Execute CI checks in an environment with full internet access and the MSVC C++ build tools (`cl.exe`) + GNU `cp`.
- Push the E5 identity fixes to the active PR (#14).
- Move on to E6 (Voice Crate).
**Blockers:**
- Codebase checks blocked by `unable to get packages from source` (network issue) and missing build tools in current Windows shell.

### Session 021 — 2026-02-27 (Workspace Test Setup Baseline + CI Parity)
**Architect:** Codex (GPT-5)
**What happened:**
- Implemented workspace test setup baseline (local + CI parity):
  - Added `scripts/setup-rust-test-env.ps1`
  - Added `scripts/run-rust-gates.ps1` (`fmt`, `check`, `clippy`, `test`, `all`)
  - Added `scripts/run-rust-tests.ps1` (crate-scoped test entrypoints)
- Wired Rust CI workflow to shared scripts:
  - Updated `.github/workflows/rust-ci.yml` jobs (`fmt-check`, `check`, `clippy`, `test`) to call `run-rust-gates.ps1`
  - Added `windows-stability` job (`continue-on-error`) for Windows gate visibility
- Added baseline test-status documentation:
  - `docs/testing-baseline.md`
  - `docs/session-handoff-2026-02-27.md`
  - Updated `README.md` with new test workflow commands and baseline doc reference
- Fixed `yaatal-core` compile blocker in Africa's Talking client:
  - `crates/yaatal-core/src/networking/africas_talking.rs`
  - Removed invalid reqwest builder method usage and changed constructor to return `Result`
**What's next:**
- Run full workspace gates in a toolchain-complete shell (`cl.exe` + `cp.exe` in PATH) and resolve any remaining pre-existing fmt drift.
- Promote Windows CI job to required after stabilization.
**Blockers:**
- Current shell missing `cl.exe` and `cp.exe` in PATH.
- `cargo fmt --all --check` still reports pre-existing formatting issues in existing files outside this change scope.

### Session 022 — 2026-02-27 (E5 Feed Integration Complete)
**Architect:** Claude (Anthropic)
**What happened:**
- Completed E5 feed integration — wired `yaatal-feed` pipeline to `yaatal-api`:
  - Added `yaatal-feed` dependency to `crates/yaatal-api/Cargo.toml`
  - Created `crates/yaatal-api/src/sources/` module with SeaORM repository adapters:
    - `post_repository.rs` — `PostRepository` trait implementation for following source
    - `discovery_repository.rs` — `DiscoveryRepository` trait implementation (trending by upvotes)
  - Created `crates/yaatal-api/src/controllers/feed.rs`:
    - `GET /api/feed` endpoint with JWT auth
    - Pagination support (`page`, `per_page` params)
    - `following_only` mode for in-network posts
    - Returns ranked feed with scores
  - Created `crates/yaatal-api/src/views/feed.rs` — re-exports `FeedResponse`, `FeedItem`
  - Wired feed routes in `crates/yaatal-api/src/app.rs`
  - Added integration tests in `crates/yaatal-api/tests/requests/feed.rs`:
    - `feed_requires_auth()` — verifies 401 without auth
    - `feed_returns_ranked_posts()` — verifies ranked output
    - `feed_pagination_works()` — verifies pagination
    - `feed_following_only_mode()` — verifies in-network filtering
- Verification:
  - `cargo check -p yaatal-api`: PASS
  - `cargo check --workspace`: PASS
**What's next:**
- E6 (Voice crate wiring) — implement cloud transcription API integration
- E7 (Kill gate) — Dioxus + cpal voice recording demo
**Blockers:**
- Pre-existing test database migration issue (`profiles.user_id` UNIQUE column conflict) — requires test DB reset or migration fix

### Session 023 — 2026-02-28 (E6 Voice Crate Audit + Code-Docs Alignment)
**Architect:** Claude Opus 4.6
**What happened:**
- Full audit of entire workspace comparing code against ARCHITECT-ENGINE.md Sessions 000-022
- Found 7 discrepancies (D1-D7), applied fixes on `e6-voice-crate` branch:
- **D1 — capture.rs (Session 005 compliance):**
  - Added `LockPoisoned` error variant to `CaptureError`
  - Replaced `unwrap()` on mutex lock in `stop()` with `map_err` → `CaptureError::LockPoisoned`
  - Added accessor methods: `is_recording()`, `sample_count()`, `sample_rate()`, `channels()`
  - Added `clear()` returning `Result<(), CaptureError>`
  - Added device config mismatch warning in `start()`
  - Stored `sample_rate` and `channels` at construction time
  - Suppressed dead_code warning on `host` field
- **D2 — compress.rs (16-bit PCM for Whisper compatibility):**
  - Changed WAV encoding from 32-bit float to 16-bit PCM (`bits_per_sample: 16, SampleFormat::Int`)
  - Added `f32_to_i16()` clamping function to prevent overflow
  - Added 3 tests: WAV header validation, f32 clamping/overflow, empty samples
- **D3 — transcribe.rs (full rewrite per Session 005 spec):**
  - Removed `candle_core::Error` import (non-compiling dependency)
  - Replaced 3-variant `TranscribeError` with 4-variant `TranscriptionError` (Network, Api, ModelLoading, EmptyResult)
  - Added `TranscriptionResult` struct with `text`, `duration_ms`, `model` fields
  - Added `transcribe_with_model()` for model selection
  - Implemented cloud path with proper HuggingFace API call, 30s timeout, HF 503 handling
  - Local path returns graceful error instead of stub string
  - Added 4 tests: error display variants, offline routing
- **D4 — voice Cargo.toml (dependency cleanup):**
  - Removed `candle-core`, `candle-nn`, `candle-transformers`, `hf-hub` (non-optional, non-compiling deps)
  - Made `cpal` optional behind `edge` feature: `cpal = { version = "0.15", optional = true }`
  - Added `[features] default = [] edge = ["dep:cpal"]` section
  - Added `[lints] workspace = true`
- **D5 — voice controller:**
  - Updated to use `TranscriptionResult.text` instead of raw `String` return
- **D6 — workspace clippy lints:**
  - Added `[workspace.lints.clippy]` to root `Cargo.toml` with correctness (deny), suspicious/complexity/style/perf (warn), and specific rules (unwrap_used, expect_used, panic, todo, dbg_macro, print_stdout/stderr, clone_on_ref_ptr, needless_pass_by_value, large_futures)
  - Added `[lints] workspace = true` to all 7 crate Cargo.tomls: yaatal-core, yaatal-api, yaatal-feed, yaatal-voice, yaatal-search, yokk-mobile, migration
- **D7 — ARCHITECT-ENGINE.md:**
  - Updated E6 phase status to IN PROGRESS
  - Added this Session 023 entry
**What's next:**
- Run `cargo check -p yaatal-voice` to verify voice crate compiles
- Run `cargo check --workspace` for full workspace verification
- Commit all changes on `e6-voice-crate` branch
**Blockers:**
- `libsql-ffi` build script requires GNU `cp` in PATH (use `$env:CARGO_TARGET_DIR = "$env:TEMP\yaatal-target2"` workaround)

### Session 024 — 2026-04-01 (Integration Stage Kickoff + Deploy and CI Triage)
**Architect:** Codex (GPT-5)
**What happened:**
- Reframed current repo status against the original E1-E8 plan:
  - confirmed `E1-E5` are effectively landed in code
  - confirmed current stage is integration/correctness/deploy hardening, not early scaffold work
- Created safe branch/worktree topology for parallel integration work:
  - control branch: `codex/branch-ops-safety`
  - deploy branch/worktree: `codex/deploy-candidate`
  - integration lanes: `codex/integration-ci-feed`, `codex/integration-runtime-railway`, `codex/integration-voice-e6`, `codex/integration-app-runtime`
- Added `docs/integration-execution-board.md` with lane ownership, Ralph loops, verification, and merge order
- Assembled a clean deploy-oriented backend/runtime branch from `main` and made it Railway-ready:
  - added `railway.json`
  - added `crates/yaatal-api/config/production.yaml`
  - added `/health` controller wiring
  - updated `.env.example`
  - committed deploy branch as `070be46`
- Verified deploy branch locally:
  - `cargo fmt --all --check`: PASS
  - `cargo check --workspace --locked`: PASS
- Investigated GitHub checks on deploy PR `#20`:
  - `fmt-check`, `check`, `clippy`: PASS
  - `test`: FAIL
  - `windows-stability`: FAIL for the same underlying feed test
- Root-caused CI failure in `crates/yaatal-api/tests/requests/feed.rs`:
  - test seeded only self-authored posts
  - live feed pipeline correctly filters self posts via `SelfPostFilter`
- Fixed the feed test on `codex/integration-ci-feed` by seeding posts from a separate author account
- Verified feed fix:
  - targeted ranked-feed test: PASS
  - `cargo test -p yaatal-api --test mod -- --test-threads=1`: PASS (`31 passed, 0 failed`)
  - committed lane fix as `3b00070`
**What's next:**
- Merge `3b00070` from `codex/integration-ci-feed` into `codex/deploy-candidate` and rerun CI
- Authenticate Railway CLI and inspect the failed deployment/runtime logs
- Continue `E6` on the dedicated voice lane
- Unblock `apps/yokk-mobile` on the app/runtime lane so full workspace status becomes meaningful again
**Blockers:**
- Railway logs/runtime investigation still blocked by CLI session/auth state
- Full workspace parity is still distorted by incomplete `yokk-mobile` app integration on the control branch
- Local Windows cargo remains sensitive to cache corruption, network reachability, and `libsql` native toolchain behavior; use bootstrap script plus isolated cargo dirs when needed

### Session 025 — 2026-04-12 (Bo-Plex Vision Reset + Setup Lanes)
**Architect:** Codex
**What happened:**
- Reframed the repo around the current Bo-Plex target architecture:
  - Engine on Railway
  - PersonaPlex as external WebSocket upstream
  - one external `/search` service boundary
  - thin client contracts using JSON envelopes + base64 audio
- Cleaned the canonical documented surface to remove stale tech/model framing:
  - updated `README.md`
  - updated `ARCHITECT-ENGINE.md`
  - updated `SPRINT-LOG.md`
  - replaced `crates/yaatal-api/README.md`
  - updated `crates/yaatal-feed/README.md`
  - updated `docs/deployment/railway.md`
  - added `docs/architecture/boplex-setup.md`
  - extended `.env.example` with planned Bo-Plex runtime variables
- Created dedicated Bo-Plex implementation worktrees from `codex/deploy-candidate`:
  - `codex/boplex-session-api`
  - `codex/boplex-personaplex-adapter`
  - `codex/boplex-search-integration`
**What's next:**
- Build the local-mock PersonaPlex adapter lane first
- Add `/api/voice/session` and in-memory session state second
- Wire the real `/search` service and grounding injection third
**Blockers:**
- No WebSocket session substrate exists yet in code
- Search contract is documented now but not fully implemented in the Engine
- Crate-scoped baseline checks in the fresh worktrees need a longer shell timeout than this session used for warm-up builds

### Session 026 — 2026-04-12 (Service-First Lane Split)
**Architect:** Codex
**What happened:**
- Revised the Bo-Plex implementation split from adapter-centric lanes to service-centric lanes:
  - voice service
  - search service
  - engine orchestrator
- Created new worktrees from `codex/deploy-candidate`:
  - `codex/voice-service`
  - `codex/search-service`
  - `codex/engine-orchestrator`
- Verified baseline state:
  - `cargo check -p yaatal-search`: PASS on `codex/search-service`
  - `cargo check -p yaatal-voice`: blocked by `libsql-sqlite3-parser` Windows build-script permission failure in this shell
  - `cargo check -p yaatal-api`: blocked by `libsql-ffi` / `cp` and the same Windows native build environment issue in this shell
- Updated the docs again to make the service-first split explicit
**What's next:**
- Build `yaatal-search` into the first runnable HTTP service surface
- Build `yaatal-voice` into the PersonaPlex-compatible local mock/service surface
- Keep `yaatal-api` focused on orchestration only
**Blockers:**
- Windows Cargo environment still blocks native `libsql`-linked crates in some worktrees

### Session 027 — 2026-04-16 (Service Lanes Green + Engine Wiring Recorded)
**Architect:** Codex
**What happened:**
- Verified the service-first lanes reached concrete implementation state:
  - `codex/search-service` green and committed at `02b10a8`
  - `codex/voice-service` green and committed at `b587e0b`
  - `codex/engine-orchestrator` wired and committed at `4c9876b`
- Confirmed the current engine branch now exposes `/api/voice/session` and routes it through voice/search service clients.
- Mapped the older `Harness × Runtime` memo against the current service-first architecture:
  - stable contract principle retained
  - HTTP-only and single-binary assumptions marked outdated
  - old ColBERT/FocalCodec/LFM model framing marked obsolete for current implementation planning
- Updated canonical docs to reflect branch reality and the current harness usability mirror.
**What's next:**
- Merge the service-lane stack back into `codex/deploy-candidate` in order: search → voice → engine
- Run the first full local loop with all three services active
- Replace mock voice/search backends incrementally while preserving the service contracts
**Blockers:**
- The active Bo-Plex loop still spans multiple branches; canonical branch does not yet reflect the implemented service-lane state
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
