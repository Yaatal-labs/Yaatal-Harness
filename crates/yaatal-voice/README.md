# yaatal-voice

`yaatal-voice` is no longer just a batch transcription helper in the target architecture.

Current branch reality:

- on `codex/deploy-candidate`, this crate still looks transitional
- on `codex/voice-service` at `b587e0b`, it already becomes the first **voice service surface** for Bo-Plex

That service lane includes:

- PersonaPlex-compatible local mock for development
- transport/client adapter for real upstream PersonaPlex later
- audio/frame utilities
- existing batch transcription fallback kept intact during the transition

## What belongs here

- frame codecs
- upstream WebSocket client logic
- local mock service behavior
- audio conversion/compression helpers

## What does not belong here

- JWT auth
- session registry
- product/business orchestration
- search routing
- feed/domain policy

Those stay in `yaatal-api`.

## Runnable surface

On the service lane, this crate now includes:

```text
src/bin/personaplex_mock.rs
```

That service lets the Engine and a thin UI exercise the vocal loop locally before RunPod is introduced.

## Harness fit

The older harness voice stub is not directly enough aligned to replace this service:

- it lacks a WebSocket session model
- it lacks streaming subtitle/audio events
- it lacks turn boundaries and context injection

So the current `yaatal-voice` service contract should be treated as the authoritative outer shape.
