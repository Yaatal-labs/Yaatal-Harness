# Yaatal Harness

**Yaatal-Harness is the AI control plane of Yaatal.** It is where agent runtimes get their
capabilities, where AI behavior becomes auditable, where policy is enforced at run time, and where
the system's own metrics and feedback flow back in to make Yaatal self-improving. This is
AI-native software: the control loop is the product, not an add-on to it.

## Charter: four functions

1. **Runtime custody.** Agent runtimes — Claw/zeroclaw-style agents, Hermes, cron-driven headless
   agents, Studio's agent loop — never get raw access to Yaatal. They plug into the Harness, which
   is the only door: every model call, tool execution, and memory access goes through Harness
   contracts (`yaatal-models`, `yaatal-tools`, `yaatal-memory`, `yaatal-search`).
2. **Trust & audit.** `yaatal-observability` and `yaatal-evals` exist to make AI behavior
   inspectable: every AI action should have a traceable audit record and a measurable quality
   score. Audit is a first-class contract here, not logging bolted on afterward.
3. **Policy & alignment enforcement.** `yaatal-policy` is the standing rulebook every runtime
   passes through — what an agent may touch, spend, say, store. This is complementary to the
   Engine, not redundant with it: the Engine enforces **data** policy at compile time (its
   sovereignty type system); the Harness enforces **behavioral** policy at run time.
4. **The self-improvement loop.** This is the point of the other three. Metrics, evals, and
   feedback flow back in, and the Harness adjusts the system — routing, prompts, tool selection,
   configuration — with policy as the guardrail: audit → metrics → evals → adjustment → audit
   again.

## Current Status

This repository is a Rust workspace scaffold that compiles with:

```powershell
cargo check --workspace --all-targets
```

The compiled workspace members are:

- `yaatal-core`: shared contracts and domain types
- `yaatal-search`: retrieval, enrichment, reranking, and streaming search
- `yaatal-models`: model provider adapters and test/mock providers
- `yaatal-tools`: tool execution contracts and prototype local tools
- `yaatal-memory`: in-memory memory store
- `yaatal-policy`: policy implementations
- `yaatal-feed`: feed/recommendation pipeline scaffold
- `yaatal-voice`: voice pipeline contracts and mock implementations
- `yaatal-observability`: tracing helpers
- `yaatal-evals`: evaluation scaffold

`yaatal-api` is still present in the repository as a temporary integration stub, but it is
intentionally excluded from the compiled workspace.

**What exists now (built + tested; see `docs/CONTROL-LOOP.md` and `docs/OPS-RUNNER.md`):**

- Persistent audit: `yaatal-audit` — `AuditEvent`, JSONL + in-memory stores, metrics rollups.
- Custody: `yaatal-policy::tool_policy` (allowlist + per-run spend cap) and
  `yaatal-tools::audited_exec` (policy-check-before-spawn, timeout kill, one audit event per
  invocation).
- Eval + proposals: `yaatal-evals::ops_run` scores each run; `yaatal-audit::proposals` generates
  L1 `ConfigProposal`s over recent runs.
- A first tenant: `yaatal-runner` (`yaatal-ops-runner`) executes a runbook through the custody
  path daily, then syncs proposals to the Engine's review API (`/api/harness/proposals`) where a
  human approves or rejects (`yaatal-proposals-push` is the on-demand CLI for the same push).

**What does not exist yet — stated plainly:**

- No general runtime adapters. There is no Claw integration and no Hermes integration; the ops
  runner is the only tenant of the custody path so far.
- No model-driven planning. The runner executes a fixed runbook; nothing plans actions with an
  LLM yet.
- No auto-apply (L2). Approved proposals are applied by a human; the loop suggests, it does not
  act on its own suggestions.

## Two runtimes, not one

Yaatal has two runtimes and they are not the same thing:

- **Yaatal Engine owns the request runtime** — Loco routes, HTTP auth, WebSocket sessions,
  profile/session identity, deployment orchestration.
- **Yaatal Harness owns the agent runtime** — execution loops, tool custody, model access, policy
  gates.

The two stay decoupled: capabilities cross the boundary as narrow HTTP/JSON contracts (e.g. the
runner pushing proposals to Engine's `/api/harness/proposals`), never as crates compiled into the
other runtime (see `ARCHITECTURE.md` § Promotion & boundary rules). Engine supplies verified
user/session/profile context; Harness supplies governed AI capability. Neither owns the other's concern: Harness does
not stand up HTTP/WebSocket entrypoints for end users, and Engine does not decide what an agent is
allowed to call or spend.

## Tool surface direction

The Harness tool surface is CLI-first: tools are exposed to agents as small, sharp CLIs
(`--json` output, clear `--help`, meaningful exit codes), executed through an auditing wrapper,
with the tool allowlist itself as policy. See `docs/CONTROL-LOOP.md` and
`docs/CLI-FIRST-TOOLS.md` for the design details.

## Integration Direction

Engine and Harness integrate through explicit HTTP/JSON contracts; neither compiles the other in.
`yaatal-tools`' dangerous local built-ins (shell, file write, git, web fetch, web search) are now
gated behind Cargo features and off by default (`default = ["safe-tools"]` — file read and
session notes only); see that crate's docs for the full feature table. Remaining near-term
cleanup should narrow the residual runtime overlap by moving the `yaatal-api` stub behind
examples, and by replacing session-owned tools with Engine-supplied adapters — all in service of
making runtime custody, audit, and policy enforcement real rather than aspirational.
