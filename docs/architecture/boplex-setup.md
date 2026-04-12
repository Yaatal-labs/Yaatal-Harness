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

## Locked approach

- **Engine is the orchestrator.** Session state, auth, routing, and retries live in `yaatal-api`.
- **Client stays thin.** Use JSON envelopes plus base64 audio over WebSocket.
- **PersonaPlex stays external.** Start with a local mock; swap to RunPod later.
- **Search stays behind one HTTP contract.** The Engine calls `/search`; BGE-M3 and Qdrant stay behind that service.
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

### Engine ↔ PersonaPlex

Thin transport adapter in `yaatal-voice`:

- upstream connect/disconnect
- send audio/control frames
- receive audio/text/turn-end/error frames

Raw `0x01` / `0x02` details stay at the adapter edge. They do not become the Engine-wide contract.

### Engine ↔ Search service

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

The Engine formats the top results into one compact grounding block and injects it back into PersonaPlex as a text context message.

## Worktree split

This is not a one-session implementation. The repo now has dedicated worktrees from `codex/deploy-candidate`:

| Worktree | Branch | Responsibility |
|----------|--------|----------------|
| `.worktrees/boplex-session-api` | `codex/boplex-session-api` | `yaatal-api` WebSocket route, JWT auth, session state, event normalization |
| `.worktrees/boplex-personaplex-adapter` | `codex/boplex-personaplex-adapter` | `yaatal-voice` PersonaPlex adapter, frame codec, local mock server |
| `.worktrees/boplex-search-integration` | `codex/boplex-search-integration` | `/search` client, grounding formatter, integration tests and failure handling |

Recommended merge order:

1. PersonaPlex adapter + mock
2. API session route + in-memory session state
3. Search injection and end-to-end vocal loop tests

## First usable milestone

The first milestone is successful when:

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
