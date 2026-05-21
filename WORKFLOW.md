---
name: yaatal-harness-internal-rd
mode: manual-symphony-lite
status: internal-rd-only
tracker:
  kind: feature_list
  source: feature_list.json
  active_statuses:
    - todo
    - in_progress
  terminal_statuses:
    - done
    - blocked
workspace:
  isolation: one_feature_per_run
  root: .
  branch_prefix: codex/
agent:
  max_parallel_sidecars: 3
  default_verification: ".\\init.ps1 -Mode all"
  proof_required: true
safety:
  product_surface: false
  dangerous_tools: feature_gated
  engine_runtime_changes: explicit_only
handoff:
  progress_log: progress.md
  feature_tracker: feature_list.json
---

# Yaatal-Harness Internal R&D Workflow

This workflow is a lightweight Symphony-style contract for improving Yaatal-Harness. It is not a BoPlex product surface and it is not a multi-tenant runner. Its purpose is to make Harness development faster, isolated, resumable, and verifiable.

## Operating Boundary

Yaatal-Harness owns reusable AI contracts and pipelines: model, search, feed, voice contracts, tools, memory, policy, evals, and observability.

Yaatal Engine owns runtime concerns: auth, Loco routes, WebSockets, deployment, user identity, profiles, session transport, and production adapters.

This workflow only coordinates Harness R&D work. If a task needs Engine changes, record the boundary explicitly before editing anything outside Harness.

## Tracker

Use `feature_list.json` as the local issue tracker.

Each run selects exactly one feature item unless the user explicitly asks for roadmap maintenance. A feature can move to `done` only when its acceptance criteria are met and verification evidence is recorded.

Recommended status meanings:

- `todo`: accepted backlog item, not started.
- `in_progress`: current run owns this item.
- `done`: implementation and verification evidence recorded.
- `blocked`: cannot continue without an external dependency or user decision.

## Run Lifecycle

1. Read `AGENTS.md`, `WORKFLOW.md`, `feature_list.json`, and `progress.md`.
2. Select one feature and state the success criteria.
3. Identify blocking work that must stay local and sidecar research or verification work that can run in parallel.
4. Dispatch subagents only for bounded sidecar tasks with clear ownership.
5. Implement the smallest change that satisfies the selected feature.
6. Run targeted checks first, then the repo gate when practical.
7. Update `feature_list.json` and `progress.md` with proof of work.
8. Leave the next recommended feature explicit.

## Subagent Pattern

Use subagents to speed up exploration, verification, and independent implementation slices. Do not delegate the immediate blocking task when the main run cannot move without it.

Recommended roles:

- `orchestrator`: owns feature selection, local critical path, integration, and final evidence.
- `explorer`: inspects local code or external primary references and returns concise findings.
- `worker`: edits a disjoint file or module slice when implementation can proceed independently.
- `verifier`: checks a specific risk after implementation starts.

Every subagent assignment should name:

- the feature id,
- the owned files or read-only scope,
- the expected output,
- what it must not change.

## Proof Of Work

A completed run should record:

- feature id and status,
- files changed,
- commands run,
- test results,
- skipped commands with reason,
- remaining risks,
- next recommended feature.

## Verification

Preferred full gate:

```powershell
.\init.ps1 -Mode all
```

Targeted gates are allowed before the full gate:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace -- --test-threads=1
```

Feature-specific checks should be recorded when they matter, for example feature combinations in `yaatal-tools`.

## Stop Conditions

Stop and ask for direction when:

- the selected feature conflicts with Harness versus Engine boundaries,
- a required external credential or service is unavailable,
- verification failure points to unrelated existing breakage,
- the task requires changing product runtime behavior.

## Not Yet Product

Do not build dashboards, ticket sync, auto-merge, or a long-running daemon until this workflow proves useful for internal Harness development. A future runner can be considered after the workflow repeatedly improves throughput and evidence quality.
