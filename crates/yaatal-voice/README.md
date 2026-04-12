# yaatal-voice

`yaatal-voice` is no longer just a batch transcription helper in the target architecture.

Its next role is to become the **voice service surface** for Bo-Plex:

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

## Intended runnable surface

This crate should grow a runnable service binary, for example:

```text
src/bin/personaplex_mock.rs
```

That service should let the Engine and a thin UI exercise the vocal loop locally before RunPod is introduced.
