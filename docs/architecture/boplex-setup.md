# Bo-Plex Setup

## Summary

Bo-Plex setup is the first real-time vocal loop for Yaatal Engine.

The goal is not a finished product. The goal is one hosted, testable orchestration loop:

- client streams audio to the Engine
- Engine brokers the session to PersonaPlex
- Engine watches the upstream text stream
- Engine calls one real `/search` service when grounding is needed
- Engine injects grounded text context back into the live session

This milestone intentionally excludes Redis, SigLIP2, generalized tool routing, ZeroClaw, and Path B model orchestration.
It also treats `yaatal-voice` and `yaatal-search` as **independently runnable service surfaces**, not just internal helper crates.

## Locked approach

- **Engine is the orchestrator.** Session state, auth, routing, and retries live in `yaatal-api`.
- **Client stays thin.** Use JSON envelopes plus base64 audio over WebSocket.
- **PersonaPlex stays external.** Start with a local mock; swap to RunPod later.
- **Search stays behind one HTTP contract.** The Engine calls `/search`; BGE-M3 and Qdrant stay behind that service.
- **Voice and search each get their own runnable surface.** Build usability by making them directly testable before the Engine owns the whole loop.
- **Grounding goes upstream as text context.** No generalized tool-call framework in milestone 1.

## Interfaces

### Client ↔ Engine

`GET /api/voice/session` WebSocket in `yaatal-api`.

Client message types:

- `session_config`
- `audio_chunk`
- `client_ping`

Engine message types:

- `session_ready`
- `subtitle`
- `audio_chunk`
- `warning`
- `error`

The client contract should stay JSON-shaped even when audio payloads are base64. The internal engine should convert that envelope into typed session events immediately.

### Voice service ↔ PersonaPlex

`yaatal-voice` should expose a runnable service surface first:

- local PersonaPlex-compatible mock server
- upstream connect/disconnect support
- audio/control frame send path
- audio/text/turn-end/error frame receive path

Raw `0x01` / `0x02` details stay at the `yaatal-voice` adapter edge. They do not become the Engine-wide contract.

### Search service contract

`POST /search`

Request:

```json
{
  "query": "white fabric near Sandaga",
  "top_k": 3,
  "lang": "wo",
  "market": "SN-DKR"
}
```

Response:

```json
{
  "hits": [
    {
      "id": "merchant-123",
      "text": "White basin fabric, 6 yards",
      "score": 0.93,
      "source": "merchant_catalog",
      "metadata": {
        "merchant": "Awa Textiles",
        "price": "12000 XOF",
        "location": "Sandaga"
      }
    }
  ]
}
```

`yaatal-search` should own the runnable HTTP surface for this contract. The Engine formats the top results into one compact grounding block and injects it back into PersonaPlex as a text context message.

## Worktree split

This is not a one-session implementation. The repo now has dedicated service-first worktrees from `codex/deploy-candidate`:

| Worktree | Branch | Responsibility |
|----------|--------|----------------|
| `.worktrees/voice-service` | `codex/voice-service` | runnable PersonaPlex-compatible local mock and `yaatal-voice` transport surface |
| `.worktrees/search-service` | `codex/search-service` | runnable `POST /search` HTTP service and `yaatal-search` contract |
| `.worktrees/engine-orchestrator` | `codex/engine-orchestrator` | `yaatal-api` WebSocket session route, JWT auth, per-turn state, service orchestration |

Recommended merge order:

1. search service
2. voice service
3. engine orchestrator

## First usable milestone

The first milestone is successful when:

- the search service can run locally and answer `POST /search`
- the voice service can run locally as a PersonaPlex-compatible mock
- authenticated client can open `/api/voice/session`
- Engine can proxy audio to a local mock PersonaPlex service
- Engine can accumulate transcript text for a turn
- one commerce-search utterance triggers exactly one `/search` request
- Engine injects grounded text context upstream
- session stays alive if `/search` fails
- existing `POST /api/voice/transcribe` still works as fallback/batch

## Out of scope for now

- Redis-backed session coordination
- SigLIP2 image embedding path
- generalized tool registry
- action workflows like booking/payments/CRM writes
- client-direct model access
- Dioxus/mobile-first implementation work
