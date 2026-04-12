# yaatal-api

`yaatal-api` is the public orchestration surface for Yaatal Engine.

It already owns the deployed backend path on Railway. The next step is to grow it from a REST backend with a batch voice endpoint into the **Bo-Plex session orchestrator**.

## What it owns today

- `GET /health`
- auth routes
- posts/comments routes
- `GET /api/feed`
- `POST /api/voice/transcribe`
- offline placeholder routes

## What it will own next

- `GET /api/voice/session` WebSocket endpoint
- JWT-authenticated session lifecycle
- per-session text buffer and turn state
- HTTP calls to the external `/search` service
- context injection back into the active PersonaPlex session

The Engine layer belongs here because session orchestration depends on:

- auth and identity
- request/session logging
- API contracts
- retries, timeouts, and failure handling

That logic should not live in `yaatal-voice`.

## Runtime shape

```text
Client UI
  ↕ WebSocket + JSON envelopes
yaatal-api
  ↕ WebSocket
PersonaPlex

yaatal-api
  ↕ HTTP
/search service
```

## Local run

```bash
cargo run -p yaatal-api --bin yaatal_api-cli -- start
```

When run from the workspace root, the binary auto-detects `crates/yaatal-api/config`.

## Notes

- `POST /api/voice/transcribe` stays as fallback/batch infrastructure during Bo-Plex setup.
- Feed and future voice grounding remain separate surfaces. `yaatal-feed` is a reusable ranking engine, not the live session orchestrator.
