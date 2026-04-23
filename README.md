# Yaatal Harness

Reusable AI capability harness for Yaatal systems.

The Harness owns model, search, memory, tool, policy, evaluation, and observability contracts. Runtime concerns such as Loco routes, authentication, WebSocket sessions, profile identity, and deployment orchestration belong in Yaatal Engine.

## Current Status

This repository is a Rust workspace scaffold that now compiles with:

```powershell
cargo check --workspace --all-targets
```

The current crates are:

- `yaatal-core`: shared contracts and domain types
- `yaatal-search`: retrieval, enrichment, reranking, and streaming search
- `yaatal-models`: model provider adapters and test providers
- `yaatal-tools`: tool execution contracts and prototype local tools
- `yaatal-memory`: in-memory memory store
- `yaatal-policy`: policy implementations
- `yaatal-feed`: feed/recommendation pipeline scaffold
- `yaatal-voice`: voice pipeline contracts and mock implementations
- `yaatal-observability`: tracing helpers
- `yaatal-evals`: evaluation scaffold
- `yaatal-api`: temporary API stub, not the intended Engine integration boundary

## Integration Direction

Engine should depend on Harness, not the other way around. Engine provides verified user/session/profile context and calls Harness pipelines through explicit Rust contracts.

Near-term cleanup should narrow runtime overlap by moving API stubs and dangerous local tools behind examples or feature flags, and by replacing session-owned tools with Engine-supplied adapters.
