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
- [ ] Push the deploy branch updates and keep Railway bootstrap as the default recovery path

## Day 6 - 2026-02-19 (Skills + Rust CI Hardening)
**Goal:** Align repository workflow with local model-agnostic skills system and enforce Rust gates in CI
**Status:** DONE
**Completed:**
- [x] Added local skill registry and canonical agent entry docs (`skills/`, `AGENTS.md`, `CLAUDE.md`, `CODEX.md`)
- [x] Added skills path validator (`scripts/validate-skills-manifest.ps1`) and CI workflow
- [x] Added skill document schema validator (`scripts/validate-skill-docs.ps1`) and CI workflow
- [x] Added Rust CI workflow (`.github/workflows/rust-ci.yml`) with `fmt`, `check`, `clippy`, `test`
- [x] Added docs: `docs/agent-usage.md`, `docs/dev-workflow-status.md`
- [x] Ran local validators successfully
- [x] Applied formatting (`cargo fmt --all`) and confirmed `cargo fmt --all --check` passes
- [x] Installed and validated MSVC toolchain availability
- [x] Ran and passed `cargo check --workspace` (with temp Cargo dirs outside OneDrive)
- [x] Ran and passed `cargo clippy --workspace --all-targets -- -D warnings`
- [x] Ran and passed `cargo test --workspace` (37 tests)
- [x] Documented Windows caveats (`OneDrive` writable target paths + `cp` requirement in `libsql-ffi`)
**Pending:**
- [ ] Continue E2 entity relations/integration test work
**Blockers:**
- None

## Day 7 - 2026-02-20 (ColBERT Zero-Shot Baseline + Sidecar)
**Goal:** Move `yaatal-search` from placeholder to executable zero-shot retrieval baseline
**Status:** DONE
**Completed:**
- [x] Added zero-shot retrieval scaffold in `crates/yaatal-search/src/zero_shot.rs` with `MRR@k`, `Recall@k`, `nDCG@k`
- [x] Added retrieval backend contract (`Retriever`) and baseline evaluator (`evaluate_zero_shot`)
- [x] Added Python ColBERT sidecar in `scripts/colbert_sidecar.py` (`/health`, `/index`, `/search`)
- [x] Added Rust HTTP adapter `ColbertHttpRetriever` in `crates/yaatal-search/src/python_sidecar.rs`
- [x] Updated `crates/yaatal-search/Cargo.toml` for blocking `reqwest` + `serde_json`
- [x] Added docs:
  - `docs/colbert-zero-shot.md`
  - `docs/unsloth-on-device-deployment.md`
  - `README.md` docs links
- [x] Verification:
  - `python -m py_compile scripts/colbert_sidecar.py` passes
  - `cargo fmt --all --check` passes
  - `cargo clippy -p yaatal-search --all-targets -- -D warnings` passes
  - `cargo test -p yaatal-search --offline` passes (3 tests)
**Pending:**
- [ ] Run full workspace gates once `libsql-ffi` `cp` build dependency is available in PATH
- [ ] Execute baseline metrics on real Yaatal labeled retrieval dataset
**Blockers:**
- Workspace `clippy/test` blocked by missing `cp` command required by `libsql-ffi` build script

## Day 8 - 2026-02-21 (WAXAL + Trilingual Retrieval Experiments + Handoff)
**Goal:** Execute real retrieval experiments for ColBERT and produce runnable handoff assets (scripts + notebook + visual outputs)
**Status:** DONE
**Completed:**
- [x] Installed HF workflow skills for this track (`hugging-face-datasets`, `hugging-face-model-trainer`, `hugging-face-trackio`)
- [x] Added experiment runner:
  - `scripts/run_lfm_colbert_waxal.py`
  - Includes WAXAL zero-shot, optional quick fine-tune, post-finetune eval, and metrics/report export
- [x] Added code-switch benchmark mode for WAXAL (`plain`, `codeswitch`, `both`)
- [x] Added trilingual iteration runner:
  - `scripts/run_lfm_colbert_fr_en_wo_iterations.py`
  - Evaluates `english`, `french`, `wolof`, `fr_en_wo_mix`
- [x] Added corpus builder:
  - `scripts/build_trilingual_synthetic_corpus.py`
  - Output: `data/corpus/fr_en_wo_v1` (`documents.jsonl`, `queries.jsonl`, `pairs.parquet`, `manifest.json`)
- [x] Ran WAXAL experiment and saved artifacts:
  - `artifacts/lfm_colbert_waxal/run-20260221-052711/metrics.json`
  - `artifacts/lfm_colbert_waxal/run-20260221-052711/issues_findings.md`
- [x] Ran trilingual zero-shot iteration experiment and saved artifacts:
  - `artifacts/lfm_colbert_fr_en_wo/run-20260221-054839/metrics.json`
  - `artifacts/lfm_colbert_fr_en_wo/run-20260221-054839/issues_findings.md`
- [x] Added visual/report outputs:
  - `artifacts/reports/lfm_colbert_summary.html`
  - `notebooks/lfm_colbert_test_results.ipynb`
- [x] Patched Unsloth-facing notebook for external free-GPU execution:
  - `notebooks/nb/💧_LFM2_ColBERT_350M_Inference.ipynb`
  - Includes WAXAL + code-switch + trilingual eval blocks and optional quick fine-tune cell

**Key Results:**
- WAXAL (`run-20260221-052711`):
  - `plain` zero-shot: `MRR@10 0.6388`, `Recall@10 0.6875`, `nDCG@10 0.6505`
  - `codeswitch` zero-shot: `MRR@10 0.4631`, `Recall@10 0.5375`, `nDCG@10 0.4804`
  - Post-finetune delta:
    - `plain`: `dMRR -0.0544`, `dRecall -0.0125`, `dnDCG -0.0451`
    - `codeswitch`: `dMRR +0.0100`, `dRecall 0.0000`, `dnDCG +0.0077`
- Trilingual (`run-20260221-054839`):
  - `english` MRR@10: `0.2298`
  - `french` MRR@10: `0.1939`
  - `wolof` MRR@10: `0.9900`
  - `fr_en_wo_mix` MRR@10: `0.9800`

**Pending:**
- [ ] Replace synthetic code-switch generation with real production user-code-switch query logs
- [ ] Add cross-lingual hard-negative mining for EN/FR -> WO retrieval
- [ ] Run full Unsloth GPU training loop in hosted notebook environment and compare against local quick-run baselines

**Blockers:**
- Local Python 3.13 runtime incompatibility for `pylate` dependency chain; Python 3.12 workaround required.
- Local Unsloth route still requires CUDA-wheel profile alignment; full run intended for hosted free-GPU notebook.

## Day 9 - 2026-02-21 (E2 Closeout Assignment Package)
**Goal:** Prepare a ready-to-assign E2 closeout package with exact implementation scope and acceptance gates
**Status:** DONE
**Completed:**
- [x] Added `docs/architect-e2-closeout-brief.md`
- [x] Documented exact E2 gaps from current codebase:
  - [x] SeaORM relations missing in FK-backed models (`enum Relation {}` placeholders)
  - [x] Missing integration coverage for migration/FK/uniqueness/relation queries
- [x] Captured definition-of-done and verification gates for E2 closure
- [x] Captured full-project post-E2 queue (E3-E8) for next architect pickup
**Pending:**
- [ ] Implement E2 closeout code per `docs/architect-e2-closeout-brief.md`

## Day 10 - 2026-02-21 (E2 Execution: Relations + Integration Tests)
**Goal:** Execute E2 closeout brief by wiring SeaORM relations and adding integration tests
**Status:** DONE
**Completed:**
- [x] Implemented FK-backed SeaORM relations across model entities:
  - [x] `profile`, `post`, `comments`, `upvotes`, `launches`, `achievements`, `bo_conversations`, `bookmarks`, `user_security_keys`
- [x] Kept `feed_items` relation intentionally empty with explicit no-FK note
- [x] Added integration tests in `crates/yaatal-core/tests/e2_schema_relations.rs`:
  - [x] migration smoke test (all expected tables)
  - [x] FK enforcement test (`posts.author_id`)
  - [x] uniqueness tests (`upvotes`, `bookmarks`)
  - [x] relation/join query tests (`profile -> posts`, `post -> comments`, `comments` self-parent join)
- [x] Verification gates:
  - [x] `cargo fmt --all --check`
  - [x] `cargo clippy --workspace --all-targets -- -D warnings`
  - [x] `cargo test --workspace`
**Pending:**
- [ ] Start E3 (AI cascade router hardening)

## Day 11 — 2026-02-21 (E3 AI Cascade Router Hardening)
**Goal:** Harden AI cascade router: offline/2G gating, rate limiting, sensitivity routing, latency tracking
**Status:** DONE
**Completed:**
- [x] Added `ai/network.rs`: `NetworkCondition` enum, injectable `NetworkGate` trait
- [x] Added `ai/rate_limit.rs`: Token-bucket `RateLimiterPool` (pure `std`)
- [x] Rewrote `ai/router.rs`: data-driven `TierConfig`, 5 tiers (T1-T5), offline/2G gating, sensitivity-aware routing, real latency, `Result` constructor
- [x] Added 10 deterministic tests in `tests/e3_ai_router.rs`
- [x] All 40 tests pass, clippy clean

## Day 12 — 2026-02-21 (E4 JWT Auth / Loco SaaS Scaffold)
**Goal:** Replace yaatal-api placeholder with full Loco SaaS scaffold for JWT authentication
**Status:** DONE
**Completed:**
- [x] Researched Loco framework: starters, JWT middleware, testing, asset serving
- [x] Installed Loco CLI v0.16.3
- [x] Scaffolded SaaS app via `loco new` (SQLite, Async workers, no assets)
- [x] Integrated into workspace (removed standalone `[workspace]`, wired deps)
- [x] Fixed `include_dir!` paths with `$CARGO_MANIFEST_DIR` prefix
- [x] Configured JWT: env-var secret, 72h expiry, Bearer + Cookie fallback
- [x] Auth endpoints: register, login, verify, forgot/reset, magic link, current user
- [x] Verification: `cargo check --workspace` PASS, `cargo clippy -p yaatal-api` PASS, `cargo test -p yaatal-core` 40/40 PASS
**Pending:**
- [ ] E5: Posts CRUD + feed (Loco scaffold generators)
- [ ] Link users ↔ profiles migration (add `user_id` FK)

## Day 13 — 2026-02-22 (E5 Feed Compile Unblock + Staged Verification)
**Goal:** Unblock `yaatal-feed` after genericization drift and re-run staged engine checks
**Status:** DONE
**Completed:**
- [x] Fixed stale candidate identifier usage (`post_id` -> `id`) in feed filters
- [x] Added missing `TopKSelector::new(k)` constructor
- [x] Reworked age filtering to use `WeightConfig.max_post_age_hours` instead of removed constant
- [x] Verified feed crate:
  - [x] `cargo test -p yaatal-feed --offline` (3 tests pass)
- [x] Re-ran staged offline checks:
  - [x] `cargo test -p yaatal-core --offline` pass
  - [x] `cargo test -p yaatal-search --offline` pass
**Pending:**
- [ ] Full workspace gates in toolchain-ready shell (`cl.exe` + real `cp` executable)
- [ ] Continue E5 API integration (Post/Comment CRUD + users↔profiles link)
**Blockers:**
- `libsql-ffi` build script requires external `cp` binary in PATH; PowerShell alias is insufficient in current shell.

## Day 14 — 2026-02-22 (E5 API Auth Test Recovery + Branch Organization)
**Goal:** Recover failing API auth tests and create clean handoff branches for E5
**Status:** DONE
**Completed:**
- [x] Created/maintained split organization branches:
  - [x] `organized/e5-feed`
  - [x] `organized/e5-api-auth`
- [x] Root-caused auth 500s in request tests to missing Loco mailer template extension variants (`*.t`)
- [x] Added template compatibility files:
  - [x] `crates/yaatal-api/src/mailers/auth/welcome/{subject.t,html.t,text.t}`
  - [x] `crates/yaatal-api/src/mailers/auth/forgot/{subject.t,html.t,text.t}`
  - [x] `crates/yaatal-api/src/mailers/auth/magic_link/{subject.t,html.t,text.t}`
- [x] Committed auth fix on `organized/e5-api-auth`:
  - [x] `e22b5e4 api/auth: add .t mail templates for loco mailer compatibility`
- [x] Verification:
  - [x] `cargo test -p yaatal-api --tests --offline` passes (`23 passed, 0 failed`)
**Pending:**
- [ ] Merge/cherry-pick `e22b5e4` into primary E5 branch (`e5-posts-feed`) before continuing CRUD/profile-link work.
- [ ] Execute full workspace gates in non-restricted environment with stable crates.io access.
**Blockers:**
- Shell/network restrictions can force fragile offline cache workarounds; prefer normal dev shell for end-to-end gates.

## Day 15 — 2026-02-24 (E5 Identity Mapping Fix + Blocked Verification Handoff)
**Goal:** Fix user/profile identity mismatch in post/comment/xp paths and hand off with documented blocker
**Status:** IN PROGRESS
**Completed:**
- [x] Root-caused mismatch: auth identity (`users.pid`) was used where domain identity (`profiles.id`) is required.
- [x] Added linked profile creation on register in `yaatal-api` auth controller.
- [x] Added profile resolver service and wired write paths to resolve `profile_id` from `user_pid`.
- [x] Updated XP service to award by `user_pid` through `profiles.user_id` lookup.
- [x] Added migration bootstrap for `profiles` table in Loco migration chain.
- [x] Added request tests for profile-link creation, author-id mapping, xp updates, and missing-profile failure.
- [x] Reverted unrelated formatting-only churn and kept only scoped fix files.
- [x] Added handoff document: `docs/session-handoff-2026-02-24.md`.
**Pending:**
- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo check --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test -p yaatal-api --tests`.
**Blockers:**
- `libsql-ffi` build-script requirement: external `cp` executable not available to cargo subprocesses in this shell.
- `cl.exe` is not available in PATH in this shell.
- `cargo fmt --all --check` is currently blocked by pre-existing trailing whitespace in `crates/yaatal-api/tests/requests/auth.rs`.

## Day 16 — 2026-02-25 (Project Review & Documentation)
**Goal:** Review codebase and summarize the state of E1-E5.
**Status:** DONE
**Completed:**
- [x] Verified recent E5 fixes for user-profile identity matching.
- [x] Generated a complete project summary documenting the completed phases.
- [x] Updated ARCHITECT-ENGINE.md with Session 020 recap.
**Pending:**
- [ ] Push E5 identity fixes to GitHub PR #14.
- [ ] Transition to E6 Voice Crate hardening once tests are green in a non-restricted shell.
**Blockers:**
- Continued absence of `cl.exe` and `cp` preventing cargo build scripts from compiling `libsql-ffi`, as well as intermittent loss of connectivity to crates.io.

## Day 17 — 2026-02-27 (Workspace Test Setup Baseline + CI Parity)
**Goal:** Standardize local/CI Rust gates and improve Windows test reliability.
**Status:** DONE
**Completed:**
- [x] Added local test environment setup script:
  - [x] `scripts/setup-rust-test-env.ps1`
- [x] Added unified Rust gate runner:
  - [x] `scripts/run-rust-gates.ps1` (`fmt`, `check`, `clippy`, `test`, `all`)
- [x] Added crate-scoped test runner:
  - [x] `scripts/run-rust-tests.ps1`
- [x] Updated Rust CI workflow to use shared scripts:
  - [x] `.github/workflows/rust-ci.yml`
  - [x] Added `windows-stability` job (`continue-on-error`)
- [x] Added baseline testing documentation:
  - [x] `docs/testing-baseline.md`
  - [x] `docs/session-handoff-2026-02-27.md`
  - [x] `README.md` test workflow section + baseline link
- [x] Fixed `yaatal-core` compile blocker in Africa's Talking client:
  - [x] `crates/yaatal-core/src/networking/africas_talking.rs`
**Pending:**
- [ ] Run full workspace gates in shell with `cl.exe` + `cp.exe` available.
- [ ] Decide whether to normalize pre-existing workspace formatting drift currently reported by `cargo fmt --all --check`.
**Blockers:**
- Current shell still missing native toolchain binaries required by some dependency build scripts.

## Day 18 — 2026-02-27 (E5 Feed Integration Complete)
**Goal:** Wire `yaatal-feed` pipeline to `yaatal-api` for ranked feed endpoint.
**Status:** DONE
**Completed:**
- [x] Added `yaatal-feed` dependency to `crates/yaatal-api/Cargo.toml`
- [x] Created SeaORM repository adapters:
  - [x] `crates/yaatal-api/src/sources/mod.rs`
  - [x] `crates/yaatal-api/src/sources/post_repository.rs` — `PostRepository` for following source
  - [x] `crates/yaatal-api/src/sources/discovery_repository.rs` — `DiscoveryRepository` for trending posts
- [x] Created feed controller:
  - [x] `crates/yaatal-api/src/controllers/feed.rs` — `GET /api/feed` with JWT auth
  - [x] `crates/yaatal-api/src/views/feed.rs` — response types
- [x] Wired routes in `crates/yaatal-api/src/app.rs`
- [x] Added integration tests:
  - [x] `crates/yaatal-api/tests/requests/feed.rs` — 4 tests (auth, ranking, pagination, following-only)
  - [x] Updated `crates/yaatal-api/tests/requests/mod.rs`
- [x] Updated documentation:
  - [x] `ARCHITECT-ENGINE.md` — E5 marked DONE, Session 022 log added
- [x] Verification:
  - [x] `cargo check -p yaatal-api`: PASS
  - [x] `cargo check --workspace`: PASS
**Pending:**
- [ ] Fix pre-existing test DB migration issue (`profiles.user_id` UNIQUE column)
- [ ] Run `cargo test -p yaatal-api --tests` after DB fix
**Blockers:**
- Test database migration conflict — requires test DB reset or migration order fix

## Day 19 — 2026-04-01 (Integration Stage Kickoff + CI Feed Recovery)
**Goal:** Shift from feature buildout into structured integration work and restore deploy-branch correctness.
**Status:** IN PROGRESS
**Completed:**
- [x] Reassessed repo stage against the original E1-E8 plan:
  - [x] Confirmed `E1-E5` are materially implemented
  - [x] Confirmed current stage is integration, correctness, deploy/runtime hardening, and app completion
- [x] Created safe worktree topology for parallel execution:
  - [x] `codex/deploy-candidate`
  - [x] `codex/integration-ci-feed`
  - [x] `codex/integration-runtime-railway`
  - [x] `codex/integration-voice-e6`
  - [x] `codex/integration-app-runtime`
- [x] Added tracked execution runbook:
  - [x] `docs/integration-execution-board.md`
- [x] Assembled clean deploy-oriented backend/runtime branch from `main`
- [x] Added Railway-ready runtime files:
  - [x] `railway.json`
  - [x] `crates/yaatal-api/config/production.yaml`
  - [x] `/health` endpoint wiring
  - [x] `.env.example` updates
- [x] Verified deploy branch locally:
  - [x] `cargo fmt --all --check`
  - [x] `cargo check --workspace --locked`
- [x] Investigated GitHub Actions on deploy PR `#20`
- [x] Root-caused Rust CI failure to feed test setup drift, not feed-handler failure
- [x] Fixed `requests::feed::feed_returns_ranked_posts` by seeding non-self discovery posts
- [x] Verified request test suite on CI lane:
  - [x] `cargo test -p yaatal-api --test mod -- --test-threads=1` (`31 passed, 0 failed`)
- [x] Committed CI lane fix:
  - [x] `3b00070 test(feed): seed non-self discovery posts in ranked feed test`
**Pending:**
- [x] Merge `3b00070` into `codex/deploy-candidate`
- [ ] Push updated deploy branch and rerun Rust CI
- [x] Validate Railway deployment and Postgres link
- [ ] Continue E6 voice completion on `codex/integration-voice-e6`
- [ ] Unblock `apps/yokk-mobile` so full workspace status reflects real integration progress
**Blockers:**
- Full-workspace status is still muddied by incomplete `yokk-mobile`
- Local Windows cargo verification remains sensitive to network access and native `libsql` build requirements

## Day 20 — 2026-04-12 (Bo-Plex Vision Reset + Setup Plan)
**Goal:** Replace stale stack/docs framing with the current Bo-Plex orchestration vision and split the work into clean implementation lanes.
**Status:** IN PROGRESS
**Completed:**
- [x] Reframed the repo around the current Bo-Plex target:
  - [x] Engine on Railway
  - [x] PersonaPlex as external WebSocket upstream
  - [x] one external `/search` boundary
  - [x] thin client contracts using JSON envelopes + base64 audio
- [x] Updated the canonical documented surface:
  - [x] `README.md`
  - [x] `ARCHITECT-ENGINE.md`
  - [x] `SPRINT-LOG.md`
  - [x] `crates/yaatal-api/README.md`
  - [x] `crates/yaatal-feed/README.md`
  - [x] `docs/deployment/railway.md`
  - [x] `docs/architecture/boplex-setup.md`
  - [x] `.env.example`
- [x] Created dedicated implementation worktrees from `codex/deploy-candidate`:
  - [x] `codex/boplex-session-api`
  - [x] `codex/boplex-personaplex-adapter`
  - [x] `codex/boplex-search-integration`
**Pending:**
- [ ] Finish crate-scoped baseline verification in each new worktree with longer cargo timeouts
- [ ] Implement PersonaPlex local mock + transport adapter
- [ ] Implement `/api/voice/session` with JWT auth and in-memory session state
- [ ] Implement `/search` client + grounding injection path
**Blockers:**
- The current codebase still has only `POST /api/voice/transcribe`; no live voice-session route exists yet
- The search boundary is documented, but not yet wired into `yaatal-api`

## Day 21 — 2026-04-12 (Service-First Split for Usability)
**Goal:** Restructure Bo-Plex execution around independently runnable voice/search services plus a thinner Engine orchestrator.
**Status:** IN PROGRESS
**Completed:**
- [x] Created new service-first worktrees from `codex/deploy-candidate`:
  - [x] `codex/voice-service`
  - [x] `codex/search-service`
  - [x] `codex/engine-orchestrator`
- [x] Updated docs to reflect the new split and supersede the older adapter-centric Bo-Plex lanes
- [x] Verified `codex/search-service` baseline:
  - [x] `cargo check -p yaatal-search`
**Pending:**
- [ ] Turn `yaatal-search` into a runnable `/search` service
- [ ] Turn `yaatal-voice` into a runnable PersonaPlex-compatible local mock/service
- [ ] Keep `yaatal-api` scoped to WebSocket session orchestration and service calls only
**Blockers:**
- `codex/voice-service` is still blocked in this shell by the `libsql-sqlite3-parser` Windows build-script permission failure
- `codex/engine-orchestrator` is still blocked in this shell by `libsql-ffi` native build requirements and the same Windows environment issue

## Day 22 — 2026-04-16 (Service Lanes Green + Engine Wiring Verified)
**Goal:** Close the service-first implementation loop by making search and voice runnable and wiring both into the Engine orchestrator.
**Status:** IN PROGRESS
**Completed:**
- [x] Built the first runnable `yaatal-search` service lane
  - [x] `codex/search-service` committed at `02b10a8`
  - [x] `/health`, `/search`, and `/index/upsert` service surface exists
  - [x] `cargo fmt --all --check`
  - [x] `cargo check -p yaatal-search`
  - [x] `cargo test -p yaatal-search -- --test-threads=1`
  - [x] `cargo clippy -p yaatal-search --all-targets -- -D warnings`
- [x] Built the first runnable `yaatal-voice` mock/service lane
  - [x] `codex/voice-service` committed at `b587e0b`
  - [x] typed contracts, backend abstraction, mock backend, and runnable WebSocket server exist
  - [x] `cargo fmt --all --check`
  - [x] `cargo check -p yaatal-voice`
  - [x] `cargo test -p yaatal-voice -- --test-threads=1`
  - [x] `cargo clippy -p yaatal-voice --all-targets -- -D warnings`
- [x] Wired the Engine to both service contracts
  - [x] `codex/engine-orchestrator` committed at `4c9876b`
  - [x] added `GET /api/voice/session`
  - [x] added voice service client
  - [x] added search service client
  - [x] added per-turn grounding injection loop
  - [x] `cargo fmt --all`
  - [x] `cargo check -p yaatal-api --lib`
  - [x] `cargo test -p yaatal-api --lib -- --test-threads=1`
  - [x] `cargo clippy -p yaatal-api --all-targets -- -D warnings`
- [x] Mapped the older `Harness × Runtime` memo against the current architecture and recorded the translation in canonical docs
**Pending:**
- [ ] Merge the service lanes back into `codex/deploy-candidate` in order
- [ ] Start the search service, voice service, and engine together and prove the first full local vocal loop
- [ ] Replace mock voice/search internals with real backends while keeping the service contracts stable
**Blockers:**
- The implemented Bo-Plex loop still lives across service/worktree branches, not on the canonical branch yet
