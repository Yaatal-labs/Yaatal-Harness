# Runtime Distribution: Harness, Engine, and Apps

Yaatal-Harness is not only an R&D workspace. It has two forms:

- **R&D Harness**: proves, tests, evaluates, and documents AI capabilities before they are promoted.
- **Runtime Harness**: the curated reliability layer that Engine uses during live product execution.

The full R&D workspace does not ship into production. The stable contracts, adapters, policy checks, memory interfaces, eval-informed defaults, and observability hooks can be distributed through Engine.

## Runtime Position

```text
YOKK / BOBO / future apps
  -> product UX, community flows, social commerce, user journeys

Yaatal Engine
  -> API, auth/session bridge, app adapters, shared runtime services

Runtime Harness inside Engine
  -> reliable AI execution, routing, tools, memory, policy, observability

Models / search / voice / external tools
  -> capability providers
```

At runtime:

```text
User action in an app
  -> app calls Engine
  -> Engine invokes Runtime Harness contracts
  -> Harness selects model/tool/memory/search policy
  -> Engine returns a safe response or action to the app
```

## What Ships

Runtime Harness can ship as crate code, Engine adapters, or service-local modules. It should include only stable pieces:

- AI routing and fallback contracts.
- Tool permission gates.
- Session memory interfaces and production adapters.
- Search, voice, model, feed, and policy traits.
- Lightweight runtime checks derived from evals.
- Observability events and trace hooks.
- Safe defaults for bandwidth, latency, and offline-aware execution.

## What Does Not Ship

The following stay in R&D:

- notebooks,
- training jobs,
- raw datasets,
- experimental checkpoints,
- Symphony-lite workflow files,
- feature tracking files,
- progress logs,
- research-only eval scripts,
- dangerous prototype tools unless explicitly enabled or replaced by Engine adapters.

## Relationship To Apps

Apps own product behavior. Engine owns reusable runtime infrastructure. Harness owns reliable AI contracts.

```text
YOKK
  -> community feed, onboarding, launches, Bo AI UI, PowerSync/Supabase client behavior
  -> calls Engine for shared AI/search/voice/feed/session capabilities

BOBO
  -> social commerce, marketplace/community mechanics, seller and buyer journeys
  -> calls Engine for identity, trust, recommendations, search, AI assistant, voice, and content safety

Future apps
  -> reuse Engine services without copying app-specific YOKK or BOBO logic
```

## Promotion Rule

```text
One app needs it
  -> keep it in the app

Two apps need it
  -> promote it to Engine

Unproven AI capability
  -> keep it in R&D Harness

Proven AI reliability logic
  -> promote it to Runtime Harness
```

## Boundary Rules

- Harness should not own auth, profile identity, WebSocket transport, app routes, or deployment.
- Engine should not own YOKK-only or BOBO-only product UX.
- Apps should not duplicate core AI reliability logic.
- Runtime Harness should expose narrow contracts and safe adapters, not research scaffolding.
- R&D Harness should record assumptions, eval results, and proof-of-work before anything is promoted.

## Distribution Model

The practical distribution path is:

```text
R&D lane
  -> Harness experiment and eval
  -> stable Harness contract
  -> Engine adapter
  -> app feature
  -> product feedback
  -> back to R&D lane
```

This keeps the platform from becoming three competing systems. The split is:

- **Harness proves and hardens capabilities.**
- **Engine serves the hardened capabilities.**
- **Apps turn those capabilities into user value.**

