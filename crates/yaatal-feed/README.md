# yaatal-feed

Reusable feed/discovery ranking pipeline for Yaatal Engine. Architecture adapted from [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) (Apache-2.0).

In the current Engine vision, `yaatal-feed` remains a **generic discovery subsystem**. It is not the Bo-Plex session orchestrator, but it is still valuable as a reusable ranked-discovery surface the API can expose independently.

## What it provides

- composable feed pipeline traits
- default social timeline assembly
- generic ranking and ingestion types
- scorer/filter/selector chain for ranked social content

## Position in the current architecture

- `yaatal-api` owns request/session orchestration
- `yaatal-feed` owns ranked discovery logic
- future Bo-Plex voice grounding may consume ranked discovery results through API-layer adapters, but not by turning this crate into a voice/session engine

## Structure

```text
src/
├── pipeline/              # traits + pipeline executor
├── types/                 # ranking and ingestion surfaces
├── weights.rs             # ranking weights for the default social timeline
├── sources/               # following/discovery sources
├── filters/               # dedup/age/self/seen/blocked filters
├── scorers/               # recency, weighted engagement, author diversity
├── selectors/             # top-k selection
├── builder.rs             # default social timeline assembly
└── lib.rs
```

## Tests

```bash
cargo test -p yaatal-feed -- --test-threads=1
```
