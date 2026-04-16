# yaatal-api

`yaatal-api` is the public orchestration surface for Yaatal Engine.

It already owns the deployed backend path on Railway.

Current branch reality:

- on `codex/deploy-candidate`, it is still primarily a REST backend with `POST /api/voice/transcribe`
- on `codex/engine-orchestrator` at `4c9876b`, it already grows into the first **Bo-Plex session orchestrator**

## What it owns today

- `GET /health`
- auth routes
- posts/comments routes
- `GET /api/feed`
- `POST /api/voice/transcribe`
- offline placeholder routes

On `codex/engine-orchestrator`, it also owns:

- `GET /api/voice/session`
- WebSocket session brokering to the voice service
- per-turn text buffering and search triggering
- HTTP calls to the search service
- grounding injection back into the active upstream voice session

## Why this lives here

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
voice service (mock now, PersonaPlex-backed later)

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
- The main remaining step is not architecture discovery. It is merge-back and end-to-end local loop proof.
