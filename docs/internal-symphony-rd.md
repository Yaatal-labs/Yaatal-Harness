# Internal Symphony R&D Setup

This note maps the Symphony idea onto Yaatal-Harness development. The goal is faster Harness R&D, not a BoPlex product feature.

## Fit

Symphony-style orchestration is useful here as a development operating model:

- `feature_list.json` acts as the issue tracker.
- `WORKFLOW.md` acts as the repo-owned workflow contract.
- `progress.md` acts as the handoff and evidence log.
- `init.ps1` acts as the default verification gate.
- Codex subagents act as bounded sidecar workers for exploration, implementation slices, and verification.

## Current Scope

The current setup is manual Symphony-lite:

1. Pick one `harness-*` feature.
2. Keep the orchestrator on the critical path.
3. Dispatch subagents for independent read-only research, codebase mapping, or disjoint file edits.
4. Integrate locally.
5. Run targeted checks and the Harness gate.
6. Record proof of work in `feature_list.json` and `progress.md`.

This gives the team the useful part first: repeatable runs, isolation by feature, explicit verification, and resumable state.

## What To Avoid For Now

Do not build the following until the manual workflow has proved useful:

- long-running daemon,
- Linear or GitHub issue sync,
- dashboard,
- auto-merge,
- production BoPlex integration,
- multi-tenant controls.

Those can become an internal product only if the manual workflow repeatedly saves time and produces better verification evidence.

## Candidate Runner Later

If this becomes worth automating, the smallest runner should:

- parse `WORKFLOW.md` front matter,
- read `feature_list.json`,
- select one eligible feature,
- create or reuse an isolated worktree,
- launch a configured coding-agent command,
- stream logs into a run directory,
- run configured verification commands,
- write a proof summary back to `progress.md`.

The runner should still treat Harness as the target and should not own Engine runtime concerns.
