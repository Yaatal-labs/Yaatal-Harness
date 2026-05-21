# AGENTS.md

Yaatal-Harness is the reusable Rust AI capability harness for YAATAL systems. It owns model, search, memory, tool, policy, evaluation, voice-contract, and observability contracts. Yaatal Engine owns runtime concerns such as auth, Loco routes, WebSockets, deployment, profile identity, and session transport.

## Startup Workflow

Before changing code:

1. Read this file.
2. Read `README.md`.
3. Read `ARCHITECTURE.md`.
4. Read `WORKFLOW.md`.
5. Read `feature_list.json`.
6. Read `progress.md`.
7. Run `.\init.ps1 -Mode check` when the environment allows.

## Working Rules

- Work on exactly one `feature_list.json` item at a time.
- Use `WORKFLOW.md` as the internal Symphony-lite R&D contract.
- Keep Harness app-agnostic and runtime-agnostic.
- Do not move Engine-owned concerns into Harness.
- Prefer explicit Rust traits and narrow adapters over concrete service ownership.
- Treat shell, file-write, git, web-fetch, and search tools as dangerous unless feature-gated or supplied by Engine.
- Use subagents for bounded sidecar work only: exploration, independent implementation slices, and verification.
- Keep docs as routing maps; code, tests, and examples are the source of truth.
- Record verification evidence before claiming a feature is done.

## Verification

Use the local PowerShell gate:

```powershell
.\init.ps1 -Mode check
.\init.ps1 -Mode test
.\init.ps1 -Mode all
```

Equivalent raw commands:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace -- --test-threads=1
```

## Definition of Done

A feature is done only when:

- Implementation is complete.
- Relevant tests or examples exist.
- Verification commands have passed or the blocker is recorded.
- `feature_list.json` contains evidence.
- `progress.md` records what changed, what was verified, and what remains open.
- The repo is restartable by a fresh agent.

## End of Session

Before stopping:

1. Update `progress.md`.
2. Update `feature_list.json` if feature status changed.
3. Record failed or skipped verification.
4. Leave the next recommended feature explicit.
