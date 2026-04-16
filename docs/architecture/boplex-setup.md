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

## Current branch state

The service-first split is no longer just planned. The active branches currently sit at:

| Branch | Commit | State |
|--------|--------|-------|
| `codex/search-service` | `02b10a8` | green and committed; runnable phase-one `/search` service exists |
| `codex/voice-service` | `b587e0b` | green and committed; runnable PersonaPlex-compatible local mock/service surface exists |
| `codex/engine-orchestrator` | `4c9876b` | green on targeted `yaatal-api` gates; `/api/voice/session` now wires voice and search services into the Engine |

Important nuance:

- these states exist on the dedicated service/worktree branches
- they are not yet merged back into `codex/deploy-candidate`
- they are not on `main`

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

## Harness-x-Runtime translation

The older `Harness × Runtime` memo is still useful, but only if reinterpreted through the current service split.

Still valid:

- keep the external contract stable while internals change
- treat runtime/backend switching as config, not code rewrites
- copy architectural patterns from external runtimes, not their code structure

Needs reinterpretation:

- `YAATAL = standalone Axum binary`
  - update to: `YAATAL = service ecosystem with one Engine/orchestrator boundary`
- `/search`, `/voice`, `/feed` as all-HTTP endpoints
  - update to: search stays HTTP, voice is WebSocket session-first
- “runtime does not matter”
  - true only at the service contract layer; voice callers still must honor the typed WebSocket message contract

Obsolete:

- ColBERT / MaxSim / FocalCodec / LFM2-Audio as the assumed implementation center
- single-binary harness as the dominant deployment target
- ZeroClaw-style runtime assumptions as the main integration frame

## Harness usability mirror

If the older internal harness were updated to mirror the current Engine design, its fit would be:

| Harness area | Current fit to Engine | What would be needed |
|-------------|------------------------|----------------------|
| Search pipeline | medium-high | keep `Retriever -> Ranker -> PolicyEngine`, but hide it behind the current `POST /search` contract |
| Voice pipeline | low | add a real WebSocket session model, turn boundaries, subtitle/audio events, and context injection |
| API/runtime layer | low-medium | replace the current stub with real Axum/Loco routes and make it call the service contracts, not internal stubs |

The practical conclusion is:

- search harness ideas are reusable behind the current search service
- voice harness code is not directly reusable yet
- the current Engine/service split is the correct outer architecture even if harness-style internals are adopted later

## Out of scope for now

- Redis-backed session coordination
- SigLIP2 image embedding path
- generalized tool registry
- action workflows like booking/payments/CRM writes
- client-direct model access
- Dioxus/mobile-first implementation work
