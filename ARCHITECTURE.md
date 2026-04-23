# Yaatal AI Harness Architecture

This document provides a high‑level overview of the proposed **Rust AI Harness**
that refactors the existing Yaatal engine into a cohesive, modular system for
retrieval, ranking, recommendation and agentic tasks. The goal is to elevate
the system from a collection of loosely coupled services to a reusable
platform that orchestrates AI components in a safe, observable and
testable manner.

## Why a harness?

Modern production AI systems are no longer single models deployed in
isolation. They consist of multiple stages that perform candidate
retrieval, feature enrichment, learned scoring, policy enforcement,
diversification and assembly. A **harness** captures this pattern and
provides the runtime infrastructure to compose these stages, enforce
deadlines, propagate context and collect metrics. It allows you to plug in
models and services without rewriting orchestration logic for each use
case.

## Crate layout

The harness is split into several crates, each with a clear
responsibility:

| Crate | Description |
| --- | --- |
| **yaatal-core** | Shared domain types and traits: `RequestContext`, `Candidate`, `Retriever`, `Ranker`, `PolicyEngine`, `ModelAdapter` and more. |
| **yaatal-api** | Temporary integration stub kept for reference. Harness is not the runtime/API owner; Yaatal Engine should host HTTP/WebSocket/Loco entrypoints. |
| **yaatal-search** | Implements the search pipeline (retrieval → ranking → policy) for lexical and vector queries. |
| **yaatal-feed** | Implements the recommendation/feed pipeline with per‑user candidate generation and ranking. |
| **yaatal-voice** | Placeholder for voice/agentic pipeline integrating ASR/NLU and response generation. |
| **yaatal-models** | Provides concrete adapters for models such as embeddings and ranking. Models adhere to the `ModelAdapter` trait. |
| **yaatal-policy** | Contains policy engines that apply hard business or safety rules to ranked items. Includes an `AllowAllPolicy` and a simple filter example. |
| **yaatal-evals** | Supplies basic evaluation metrics (e.g. MRR, NDCG) for offline or online experiments. |
| **yaatal-observability** | Centralizes tracing and logging configuration for consistent observability across the harness. |

Additional crates can be added over time (e.g. `yaatal-agents` or
`yaatal-memory`) as the system evolves.

## Request lifecycle

1. **Engine Runtime**: A request arrives at Yaatal Engine, which owns
   authentication, profile/session state, and transport concerns. Engine
   constructs a `RequestContext` and selects the appropriate Harness
   pipeline (search, feed, voice) based on the route or session event.
2. **Retrieval**: The pipeline calls one or more `Retriever`
   implementations to fetch candidates. This could be a BM25 search,
   vector search or graph traversal.
3. **Ranking**: Candidates are scored using a `Ranker`, which in
   practice wraps a model adapter (e.g. a deep reranker) and may
   perform feature enrichment.
4. **Policy**: A `PolicyEngine` filters or annotates ranked items
   according to product rules (e.g. remove unsafe content) or
   regulatory requirements.
5. **Assembly**: The pipeline assembles the final response and
   returns it to Engine. Observability hooks capture latencies and
   outcomes at each stage.

## Implementing your own stages

To extend the harness you implement the traits defined in
`yaatal-core` and register them in your service:

* **Retriever**: returns `Vec<Candidate>` given a query. Could be
  backed by a search index, vector database or remote service.
* **Ranker**: converts candidates into `Vec<ScoredCandidate>`. Uses a
  `ModelAdapter` for inference.
* **PolicyEngine**: filters or annotates scored candidates.
* **ModelAdapter<I, O>**: provides a unified interface over
  different model providers (local, remote, CPU, GPU, etc.).

Pipelines coordinate these stages while preserving context and
enforcing budgets. The current `search::pipeline` and
`feed::pipeline` functions illustrate the expected structure; you can
swap in your own retrievers, rankers and policies at runtime.

## Observability and evaluation

Observability is critical when orchestrating multiple AI components.
The `yaatal-observability` crate should provide reusable tracing helpers
and event types, while the outer runtime installs the global subscriber.
The evaluation crate provides basic metrics for measuring ranking
quality; extend it with your own offline or online evaluation framework
as needed.

## Next steps

This repository is a scaffold. To turn it into a production harness
you will need to:

* Implement real retrieval and ranking adapters (e.g. integrate with
  ElasticSearch, FAISS, or a hosted LLM endpoint).
* Flesh out the `yaatal-voice` crate with ASR and NLU components.
* Add further policy engines for safety, personalization or diversity.
* Integrate Harness into Yaatal Engine through explicit Rust contracts
  rather than making Harness the API/runtime owner.
* Build admin and evaluation tooling to monitor performance.

Nevertheless, this structure establishes a clear separation of
responsibilities and provides a strong foundation for building
AI‑driven systems in Rust.
