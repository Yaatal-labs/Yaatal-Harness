# Yaatal AI Harness Architecture

This document describes the **Rust AI Harness** — the AI control plane of Yaatal. Its job is not
to be another place to write model/retrieval/ranking code; it is to be the layer every agent
runtime must pass through, so that AI behavior is governed, auditable, and able to improve itself
based on its own feedback.

## Why a harness?

Yaatal will run multiple agent runtimes over time — Claw/zeroclaw-style agents, Hermes, cron-driven
headless agents, Studio's livestream agent loop. If each of those calls models, tools, and memory
directly, there is no single place to see what AI did, no single place to say what AI is allowed to
do, and no way to feed outcomes back into the system. The Harness exists to close that gap: it is
the only door agent runtimes go through to reach model calls, tool execution, and memory/search
access, and it is where the audit trail, the policy check, and the evaluation score attach to every
one of those calls.

Modern production AI systems are also not single models deployed in isolation — they are staged
pipelines (retrieval, enrichment, scoring, policy, assembly). The Harness captures that pattern too,
but the pipeline shape is secondary to the control-plane charter: custody, audit, policy, and the
loop that lets the system adjust itself.

## Charter, restated as structure

The four functions from the README map onto the crates like this:

| Function | Crates |
| --- | --- |
| Runtime custody (the only door for agent runtimes) | `yaatal-models`, `yaatal-tools`, `yaatal-memory`, `yaatal-search` |
| Trust & audit (traceable record + quality score per action) | `yaatal-observability`, `yaatal-evals` |
| Policy & alignment (behavioral rules every runtime passes through) | `yaatal-policy` |
| Self-improvement loop (metrics/evals/feedback → adjustment) | spans all of the above; not yet implemented as a connected loop |

## Crate layout

| Crate | Description |
| --- | --- |
| **yaatal-core** | Shared domain types and traits: `RequestContext`, `Candidate`, `Retriever`, `Ranker`, `PolicyEngine`, `ModelAdapter` and more. |
| **yaatal-api** | Temporary integration stub kept for reference. Excluded from the compiled workspace; not the runtime/API owner in either the request-runtime or agent-runtime sense. |
| **yaatal-search** | Implements the search pipeline (retrieval → ranking → policy) for lexical and vector queries. One of the custody surfaces an agent runtime calls through rather than reaching a retriever directly. |
| **yaatal-feed** | Implements the recommendation/feed pipeline with per-user candidate generation and ranking. |
| **yaatal-voice** | Placeholder for voice/agentic pipeline integrating ASR/NLU and response generation. |
| **yaatal-models** | Model provider adapters, currently test/mock providers. The custody point for model calls: an agent runtime does not hold a raw API key or client, it calls through here. |
| **yaatal-tools** | Tool execution contracts and prototype local tools. The custody point for tool execution; the CLI-first tool surface (see below) is designed to sit behind this crate. |
| **yaatal-memory** | In-memory memory store today; the custody point for memory access. |
| **yaatal-policy** | Policy engines applying behavioral rules — what an agent may touch, spend, say, store. Includes an `AllowAllPolicy` and a simple filter example; this is scaffolding, not a populated rulebook yet. |
| **yaatal-evals** | Evaluation metrics (e.g. MRR, NDCG) and, in the target design, the quality-score half of "every AI action has a traceable audit record and a measurable score." |
| **yaatal-observability** | Tracing helpers; the audit-record half of trust & audit. Currently in-process tracing, not a persistent, queryable audit store. |

Additional crates can be added over time (e.g. `yaatal-agents`) as the system evolves.

## Two runtimes: where this crate layout sits

Yaatal splits into two runtimes that must not be confused:

- **Engine's request runtime**: Loco routes, HTTP auth, WebSocket sessions, profile/session
  identity, deployment. This is Yaatal Engine, a separate repository.
- **Harness's agent runtime**: execution loops, tool custody, model access, policy gates. This is
  everything in this repository.

Engine depends on Harness, never the reverse. Concretely: Engine constructs a `RequestContext`
(carrying verified user/session/profile identity) and hands it to a Harness pipeline or agent
runtime; the Harness never authenticates a user itself and never owns an HTTP/WebSocket
entrypoint for end users. In the other direction, an agent runtime running under the Harness never
reaches into Engine's database or routes directly — it only gets governed capability
(`yaatal-models`, `yaatal-tools`, `yaatal-memory`, `yaatal-search`) mediated by `yaatal-policy` and
recorded by `yaatal-observability`.

This is also the division of labor for policy: the Engine's sovereignty type system
(`Sensitivity { Sovereign | Operational | Public }`, `Tagged<T, S>`, `StorageDispatcher`) enforces
**data** policy at compile time — what storage tier a piece of data may live in. `yaatal-policy`
enforces **behavioral** policy at run time — what an agent may do with the capabilities it's given.
Neither replaces the other.

## Request / agent-action lifecycle

1. **Engine Runtime**: A request arrives at Yaatal Engine, which owns authentication,
   profile/session state, and transport concerns. Engine constructs a `RequestContext`.
2. **Entry into the Harness**: Engine calls into a Harness pipeline (search, feed, voice) or an
   agent runtime plugged into the Harness. This is the custody boundary — everything downstream of
   this point is governed by the Harness, not by whatever called it.
3. **Retrieval**: The pipeline calls one or more `Retriever` implementations to fetch candidates —
   BM25 search, vector search, graph traversal.
4. **Ranking**: Candidates are scored using a `Ranker`, which in practice wraps a `ModelAdapter`
   and may perform feature enrichment.
5. **Policy gate**: A `PolicyEngine` filters or annotates ranked items, or — for agent-runtime
   calls — decides whether a tool call, model call, or memory access is allowed at all, per the
   rulebook in `yaatal-policy`.
6. **Audit**: Observability hooks capture latencies, outcomes, and (in the target design) an audit
   record for the action at each stage. Today this is tracing only; there is no persistent audit
   store yet.
7. **Assembly**: The pipeline assembles the final response and returns it to Engine or to the
   calling agent runtime.
8. **Feedback (target, not yet built)**: Audit records and eval scores are meant to flow back into
   routing, prompt, tool-selection, and config decisions — closing the loop that makes this
   AI-native rather than AI-as-a-feature. No stage of this feedback path is wired up yet; the crates
   above hold the contracts it will eventually run through.

## Tool surface: CLI-first

Tools reach agents as small, sharp CLIs — `--json` output, clear `--help`, meaningful exit codes —
invoked through an auditing wrapper rather than called in-process, so every tool call is a
observable, policy-checked event rather than a bare function call. The tool allowlist is itself a
policy artifact. See `docs/CONTROL-LOOP.md` and `docs/CLI-FIRST-TOOLS.md` for the design; this
document only records where that surface sits in the architecture (behind `yaatal-tools`, gated by
`yaatal-policy`, recorded by `yaatal-observability`).

## Implementing your own stages

To extend the harness you implement the traits defined in `yaatal-core` and register them in your
service:

* **Retriever**: returns `Vec<Candidate>` given a query. Could be backed by a search index, vector
  database or remote service.
* **Ranker**: converts candidates into `Vec<ScoredCandidate>`. Uses a `ModelAdapter` for inference.
* **PolicyEngine**: filters or annotates scored candidates, or gates an agent-runtime action.
* **ModelAdapter<I, O>**: provides a unified interface over different model providers (local,
  remote, CPU, GPU, etc.), and is the custody seam for model access.

Pipelines coordinate these stages while preserving context and enforcing budgets. The current
`search::pipeline` and `feed::pipeline` functions illustrate the expected structure; you can swap in
your own retrievers, rankers and policies at runtime.

## Observability and evaluation

`yaatal-observability` should provide reusable tracing helpers and event types today, and grow into
a persistent, queryable audit store — the record half of trust & audit. `yaatal-evals` supplies
basic offline ranking metrics today, and is meant to grow into the quality-score half, feeding
scores back into the self-improvement loop. Neither crate is connected to a live feedback path yet.

## Next steps

This repository is a scaffold for a control plane, not yet the control plane in operation. To move
it forward:

* Build the first runtime adapter — a real Claw or Hermes integration — so at least one agent
  runtime is actually mediated by `yaatal-models` / `yaatal-tools` / `yaatal-memory` rather than
  hypothetically so.
* Replace in-process tracing in `yaatal-observability` with a persistent audit store.
* Wire real metrics ingestion from a live Engine or Studio deployment into `yaatal-evals`.
* Build the first feedback→adjustment segment (even a narrow one, e.g. routing) so the loop has at
  least one closed link: audit → metrics → evals → adjustment → audit again.
* Flesh out `yaatal-policy` from example filters into an actual behavioral rulebook (spend limits,
  tool allowlists, content rules).
* Narrow the `yaatal-api` stub's footprint further, and finish moving session-owned tools to
  Engine-supplied adapters.

Nevertheless, this structure establishes a clear separation of responsibilities — runtime custody,
audit, policy, and the loop that ties them together — and provides a foundation for making Yaatal
AI-native rather than AI-augmented.
