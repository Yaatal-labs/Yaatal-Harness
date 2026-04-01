# Integration Execution Board

## Purpose

This document sets the repository up for the next phase of work:

- integration across crate and app boundaries
- correctness and CI recovery
- runtime and deployment completion

The project is no longer mainly blocked on missing primitives. It is now blocked on integrating what already exists.

## Current Baseline

Code convergence base at setup time:

- release-oriented branch: `codex/deploy-candidate`
- base commit: `070be46`

Known baseline at setup time:

- `cargo fmt --all --check`: pass on `codex/deploy-candidate`
- `cargo check --workspace --locked`: pass on `codex/deploy-candidate`
- GitHub Rust CI: failing on `test`
- failing test: `requests::feed::feed_returns_ranked_posts`
- deploy checks: Railway and Vercel still require runtime investigation

Control plane:

- control worktree: `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine`
- control branch: `codex/branch-ops-safety`

The control plane is for orchestration, docs, branch management, and integration decisions. It is not the release branch.

## Worktree Topology

All execution lanes are isolated under `.worktrees/` and branch from `codex/deploy-candidate`.

| Lane | Worktree | Branch | Primary Goal |
|---|---|---|---|
| Control plane | `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine` | `codex/branch-ops-safety` | orchestration, docs, merge management |
| CI + feed correctness | `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-ci-feed` | `codex/integration-ci-feed` | restore green Rust CI by fixing feed/test drift |
| Runtime + Railway | `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-runtime-railway` | `codex/integration-runtime-railway` | get deploy branch production-ready and observable |
| Voice E6 | `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-voice-e6` | `codex/integration-voice-e6` | finish the E6 voice hardening path |
| App + runtime completion | `C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-app-runtime` | `codex/integration-app-runtime` | unblock app/runtime integration and workspace completion |

## Lane Ownership

### 1. CI + feed correctness

Primary files:

- `crates/yaatal-api/tests/requests/feed.rs`
- `crates/yaatal-api/src/controllers/feed.rs`
- `crates/yaatal-feed/**`
- `.github/workflows/rust-ci.yml` only if CI behavior needs a real fix

Success condition:

- Rust CI green for `fmt-check`, `check`, `clippy`, `test`
- feed behavior and feed test expectations match

### 2. Runtime + Railway

Primary files:

- `railway.json`
- `.env.example`
- `crates/yaatal-api/config/production.yaml`
- `crates/yaatal-api/src/controllers/health.rs`
- deploy/startup/runtime config files under `crates/yaatal-api`

Success condition:

- Railway build succeeds
- service starts cleanly
- `/health` responds
- required env set is documented and minimal

### 3. Voice E6

Primary files:

- `crates/yaatal-voice/**`
- `crates/yaatal-api/src/controllers/voice.rs`
- voice-related tests in `crates/yaatal-voice` and `crates/yaatal-api`

Success condition:

- E6 behavior matches intended contract
- crate checks cleanly
- voice route and crate behavior align

### 4. App + runtime completion

Primary files:

- `apps/yokk-mobile/**`
- app-facing integration code that depends on feed, voice, or auth

Success condition:

- app-side compile blockers removed
- workspace no longer fails only because of `yokk-mobile`
- app/runtime integration path is explicit enough to start E8 deliberately

## Non-Overlap Rules

Each lane should prefer a disjoint write set.

- `integration-ci-feed` owns feed correctness first
- `integration-runtime-railway` owns deploy/runtime config first
- `integration-voice-e6` owns voice crate and voice route first
- `integration-app-runtime` owns `apps/yokk-mobile` first

If a lane must cross into another lane's files:

1. minimize the edit
2. note the dependency in the commit message or handoff
3. merge the upstream lane first before continuing

## Ralph Loop

Each lane should run the same short cycle:

1. Reproduce the exact failure or missing behavior.
2. Narrow the write set before editing.
3. Implement the smallest coherent fix.
4. Run lane verification immediately.
5. Record the result and remaining gap.
6. Merge or forward-port only after verification.

This keeps parallel work fast without turning the branch graph into guesswork.

## Verification by Lane

### CI + feed correctness

Run:

```powershell
cargo test -p yaatal-api --test mod -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
```

### Runtime + Railway

Run:

```powershell
cargo check --workspace --locked
railway status
railway logs
```

Also verify the deployed service health endpoint.

### Voice E6

Run:

```powershell
cargo check -p yaatal-voice
cargo test -p yaatal-voice -- --test-threads=1
```

### App + runtime completion

Run:

```powershell
cargo check --workspace
```

Focus on clearing `apps/yokk-mobile` compile blockers first.

## Merge Order

Merge order should reduce rework:

1. `codex/integration-ci-feed`
2. `codex/integration-runtime-railway`
3. `codex/integration-voice-e6`
4. `codex/integration-app-runtime`

Target merge branch for integration convergence:

- `codex/deploy-candidate`

The control plane branch remains the place for planning and branch hygiene, not the shipping line.

## Quick Start

```powershell
git worktree list
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-ci-feed
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-runtime-railway
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-voice-e6
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\integration-app-runtime
```

## Immediate Next Moves

1. Fix the feed test/behavior mismatch in `integration-ci-feed`.
2. Re-run GitHub Actions locally where practical, then push.
3. Log into Railway in `integration-runtime-railway` and inspect the failed deployment.
4. Use the voice and app lanes only after the CI lane stops masking everything else.
