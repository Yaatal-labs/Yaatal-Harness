# yaatal-search

`yaatal-search` is now the **search service surface** for the Bo-Plex loop on the active service lane.

Current branch reality:

- on `codex/deploy-candidate`, this crate still reads as an internal retrieval/eval surface
- on `codex/search-service` at `02b10a8`, it already exposes a runnable service with:
  - `GET /health`
  - `POST /search`
  - `POST /index/upsert`

The Engine should call that contract and stay ignorant of the retrieval internals behind it.

## What belongs here

- search request/response contract types
- local HTTP service surface
- retrieval client/adapters
- evaluation helpers and benchmark tooling

## What does not belong here

- WebSocket session orchestration
- auth/session ownership
- persona logic
- direct client-facing application state

Those stay in `yaatal-api`.

## Runnable surface

On the service lane, this crate now includes a runnable service binary:

```text
src/bin/search_service.rs
```

That is the first directly testable retrieval surface for the Bo-Plex loop.

## Harness fit

If the older internal harness is reused later, this is the best place to do it:

- `Retriever -> Ranker -> PolicyEngine` can live behind the current `POST /search` contract
- the Engine should still remain blind to those internals
- BGE-M3 + Qdrant + Postgres remain the likely real backend direction
