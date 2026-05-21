# Yaatal-Harness Progress

## Current Snapshot

Yaatal-Harness is a Rust workspace scaffold for reusable AI contracts and pipelines. The current compiled workspace excludes `yaatal-api`, which matches the intended boundary: Engine owns runtime/API/session transport, Harness owns reusable contracts.

## 2026-05-20

Added initial harness-control artifacts:

- `AGENTS.md`
- `feature_list.json`
- `progress.md`
- `init.ps1`

Purpose: mirror Learn Harness Engineering best practice by making the repo agent-resumable, scoped, and verifiable across sessions.

During verification, two harness-quality issues were found and fixed:

- `init.ps1` did not fail the script when a Cargo subcommand returned a nonzero exit code.
- `yaatal-memory` doctest imported `MemoryType` from `yaatal_memory` and omitted the `MemoryStore` trait import.

Completed `harness-001` dangerous tool feature gates:

- Added `yaatal-tools` features: `default`, `safe-tools`, `file-read`, `session-note`, `local-shell`, `file-write`, `git`, `network`, `web-fetch`, `web-search`, `r-and-d-tools`, and `all-builtin-tools`.
- Made `reqwest` and `urlencoding` optional dependencies used only through `network`-backed features.
- Changed `ToolExecutor::register_builtin` from returning `()` to returning `Result<(), ToolError>`.
- Kept `file_read` and `session_note` available through the default safe build.
- Gated `shell`, `file_write`, `git`, `web_fetch`, and `search` so disabled registration returns `ToolError::PermissionDenied` naming the required Cargo feature.
- Updated docs and crate examples so default usage no longer registers or executes `shell`.
- Added default safety tests and feature-gated registration tests for R&D tools.

## 2026-05-21

Completed `harness-006` internal Symphony R&D workflow contract:

- Added `WORKFLOW.md` as the canonical manual Symphony-lite contract for Harness development.
- Added `docs/internal-symphony-rd.md` as a non-normative mapping note from Symphony concepts to Harness R&D practice.
- Updated `AGENTS.md` startup and working rules so future agents read `WORKFLOW.md` and use subagents only for bounded sidecar work.
- Recorded the setup as internal R&D infrastructure, not a BoPlex product surface or Engine runtime concern.
- Left `init.ps1` unchanged because the existing gate already fits the workflow.

Completed `harness-002` durable session memory:

- Replaced the process-local `SessionNoteTool` map with a public `SessionNoteStore` trait.
- Added `FileSessionNoteStore`, a JSON-backed store for workspace-scoped notes.
- Default `BuiltinTool::SessionNote` registration now stores notes at `.yaatal/session_notes.json` under the executor workspace.
- Session notes are keyed by `RequestContext.metadata["session_id"]`, with `request_id` as fallback.
- Added `SessionNoteTool::with_store` so Engine or future Harness adapters can supply a different store.
- Added temp-file replacement when writing the JSON store.
- Added `.yaatal/` to `.gitignore`.
- Updated docs to describe the session-note path, `session_id` behavior, and custom store hook.
- Added tests for executor reopen persistence, direct file-store reopen, and session-id isolation.

Documented runtime distribution boundary:

- Added `docs/runtime-distribution.md` to explain the split between R&D Harness and Runtime Harness.
- Defined where Runtime Harness sits inside Engine at live use time.
- Documented what ships to runtime versus what remains R&D-only.
- Clarified how YOKK, BOBO, and future apps should consume Engine without duplicating AI reliability logic.
- Linked the runtime distribution note from `docs/README.md`.

## Verification

Run after adding these files and fixes:

- `.\init.ps1 -Mode all` passed on 2026-05-20.
- This ran `cargo fmt --all --check`.
- This ran `cargo check --workspace --all-targets`.
- This ran `cargo test --workspace -- --test-threads=1`.

Run after completing `harness-001`:

- `cargo check -p yaatal-tools --no-default-features` passed on 2026-05-20.
- `cargo test -p yaatal-tools` passed on 2026-05-20.
- `cargo test -p yaatal-tools --all-features` passed on 2026-05-20.
- `cargo test --workspace -- --test-threads=1` passed on 2026-05-20.
- `.\init.ps1 -Mode all` passed on 2026-05-20.

No requested verification command was skipped. No Engine/runtime crate was changed for `harness-001`.

Run after completing `harness-006`:

- `feature_list.json` parsed successfully on 2026-05-21.
- `.\init.ps1 -Mode check` passed on 2026-05-21.

Run after completing `harness-002`:

- `cargo test -p yaatal-tools` passed on 2026-05-21.
- `cargo check -p yaatal-tools --no-default-features` passed on 2026-05-21.
- `cargo test -p yaatal-tools --all-features` passed on 2026-05-21.
- `cargo test --workspace -- --test-threads=1` passed on 2026-05-21.
- `.\init.ps1 -Mode all` passed on 2026-05-21.

No requested verification command was skipped for `harness-002`.

Run after documenting runtime distribution:

- Documentation-only change; no additional Rust gate was required after the previous `.\init.ps1 -Mode all` pass.

## Open Risks

- Git reports this worktree as dubious ownership under the sandbox user; status/log commands may require a safe-directory configuration outside this repo.
- Dangerous tools are now feature-gated, but their implementations remain R&D prototypes when explicitly enabled.
- `FileSessionNoteStore` uses a JSON file and a process-local lock. It is suitable for Harness R&D and single-process use, but not for high-volume or multi-process production writes.
- Engine must supply a stable `session_id`; otherwise notes fall back to `request_id` and become per-request.
- The Symphony-lite setup is manual. A daemon, dashboard, issue sync, and auto-merge remain intentionally out of scope until the manual workflow proves useful.

## Next Recommended Feature

Start `harness-003` Engine integration example.
