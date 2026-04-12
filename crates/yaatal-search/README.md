# yaatal-search

`yaatal-search` should become the **search service surface** for the Bo-Plex loop.

Today it already proves the sidecar/HTTP pattern. The next step is to make it directly runnable as the service behind:

```text
POST /search
```

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

## Intended runnable surface

This crate should grow a runnable service binary, for example:

```text
src/bin/search_service.rs
```

That binary becomes the first directly testable retrieval surface for the Bo-Plex loop.
