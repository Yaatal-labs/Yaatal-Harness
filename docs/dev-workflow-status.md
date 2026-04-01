# Dev Workflow Status

Date: 2026-04-01

## Summary

- The project is now in an integration-heavy stage: backend core phases are implemented, and the main work is correctness, deployment/runtime completion, voice completion, and app wiring.
- A clean deploy-oriented branch exists at `codex/deploy-candidate` and passes `fmt` plus `check`.
- The Rust CI feed-test mismatch was fixed on `codex/integration-ci-feed` and has now been merged into `codex/deploy-candidate`.
- Full workspace status is still not the best truth source because `apps/yokk-mobile` is not yet complete.

## Current Standard Commands

| Command | Purpose |
|---|---|
| `pwsh -File .\scripts\run-rust-gates.ps1 -Mode fmt` | Formatting gate |
| `pwsh -File .\scripts\run-rust-gates.ps1 -Mode check` | Compile gate |
| `pwsh -File .\scripts\run-rust-gates.ps1 -Mode clippy` | Lint gate |
| `pwsh -File .\scripts\run-rust-gates.ps1 -Mode test` | Workspace test gate |
| `pwsh -File .\scripts\run-rust-gates.ps1 -Mode all` | Full gate sequence |
| `pwsh -File .\scripts\run-rust-tests.ps1 -Scope yaatal-search` | Crate-scoped test run |

## Verification Snapshot (Latest Session)

| Command | Status | Notes |
|---|---|---|
| `cargo fmt --all --check` on `codex/deploy-candidate` | PASS | Clean deploy branch baseline |
| `cargo check --workspace --locked` on `codex/deploy-candidate` | PASS | Backend/runtime deploy branch compiles |
| `cargo test -p yaatal-api --test mod -- --test-threads=1` on `codex/integration-ci-feed` | PASS | `31 passed, 0 failed` after feed test fix |
| Feed test fix on `codex/deploy-candidate` | MERGED | Cherry-picked from `3b00070` into the deploy branch |
| GitHub Actions `Rust CI` on PR `#20` before merge | FAIL | `fmt/check/clippy` passed; the failing test was `requests::feed::feed_returns_ranked_posts` before the fix landed |
| Railway deployment | HEALTHY | Deploy path and Postgres link were verified after Railway hardening |

## Windows Caveats

1. OneDrive-backed repo paths can break cargo build outputs.
2. Native dependency builds are sensitive to:
   - `cl.exe` availability and shell/toolchain setup
   - `cp.exe` availability for certain build scripts
3. Parallel cargo invocations against the same target directory increase lock/permission risk.
4. Fresh temporary cargo homes can require network access to crates.io; isolated verification may need either a healthy cache or outside-sandbox network access.

## Recommended Local Setup (Windows)

Use the shared setup script before running gates:

```powershell
pwsh -File .\scripts\bootstrap-windows-rust-env.ps1
```

Expected prerequisites:

1. Visual Studio Build Tools with C++ workload (`cl.exe` in PATH)
2. Git for Windows utilities (`cp.exe` in PATH)
3. Workspace gates run from a single shell/session (avoid concurrent runs on same target dir)
4. Prefer isolated worktrees for integration lanes; see `docs/integration-execution-board.md`
