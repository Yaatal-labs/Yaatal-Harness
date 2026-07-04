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

**What does not exist yet — stated plainly:**

- No runtime adapters. There is no Claw integration and no Hermes integration; nothing outside
  this repo calls these contracts yet.
- No persistent audit store. `yaatal-observability` provides tracing helpers, not a durable,
  queryable audit log.
- No metrics ingestion from a live Engine or Studio. `yaatal-evals` is a scaffold of offline
  metrics, not a pipeline fed by production traffic.
- No feedback→adjustment mechanism of any kind. The self-improvement loop described above has no
  segments connected yet — this repo currently holds the contracts the loop will eventually run
  through, not the loop itself.

Treat every claim above as the actual state, not the goal: this is a compiling skeleton — contracts,
mock providers, prototype tools, an in-memory store, policy implementations, and scaffolding.

## Two runtimes, not one

Yaatal has two runtimes and they are not the same thing:

- **Yaatal Engine owns the request runtime** — Loco routes, HTTP auth, WebSocket sessions,
  profile/session identity, deployment orchestration.
- **Yaatal Harness owns the agent runtime** — execution loops, tool custody, model access, policy
  gates.

Engine depends on Harness, never the reverse. Engine supplies verified user/session/profile
context; Harness supplies governed AI capability. Neither owns the other's concern: Harness does
not stand up HTTP/WebSocket entrypoints for end users, and Engine does not decide what an agent is
allowed to call or spend.

## Tool surface direction

The Harness tool surface is CLI-first: tools are exposed to agents as small, sharp CLIs
(`--json` output, clear `--help`, meaningful exit codes), executed through an auditing wrapper,
with the tool allowlist itself as policy. See `docs/CONTROL-LOOP.md` and
`docs/CLI-FIRST-TOOLS.md` for the design details.

## Integration Direction

Engine depends on Harness through explicit Rust contracts; Harness does not depend on Engine.
Near-term cleanup should narrow the residual runtime overlap by moving the `yaatal-api` stub and
any dangerous local tools behind examples or feature flags, and by replacing session-owned tools
with Engine-supplied adapters — all in service of making runtime custody, audit, and policy
enforcement real rather than aspirational.
