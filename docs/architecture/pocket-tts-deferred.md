# Pocket TTS Deferred Implementation

Pocket TTS is intentionally deferred for the current Yaatal stage.

The current priority remains:

- Engine orchestration
- search and grounding stability
- edge/cloud routing
- local vocal loop proof against the existing service boundaries

Pocket TTS is still the preferred candidate for future local speech synthesis, but it should not expand the active scope yet.

## Placement

When resumed, Pocket TTS should be integrated as an Engine-side local service.

It should not be introduced first as:

- a Harness concern
- a browser or app-side runtime default
- a broad upstream-compatible demo surface

The Engine already owns:

- session orchestration
- edge/cloud route decisions
- voice output selection
- client-facing audio delivery

That is the correct boundary for a future TTS sidecar.

## Safe v1 Service Shape

The future YAATAL-owned Pocket TTS service should stay narrow.

Keep:

- `GET /health`
- `POST /synthesize`

Recommended request shape:

```json
{
  "text": "White fabric is available in Sandaga.",
  "voice_id": "alba",
  "format": "wav"
}
```

Recommended response behavior:

- return WAV bytes by default
- include explicit audio metadata
- keep voice resolution server-managed

## What To Exclude

The upstream project contains useful capabilities, but the first YAATAL integration should exclude:

- web UI
- WASM UI
- permissive CORS
- multipart compatibility routes
- OpenAI compatibility routes that do not fully honor the contract
- arbitrary local file-path voice resolution
- request-supplied `hf://` voice URLs
- raw base64 voice prompts from untrusted callers
- public per-request tuning knobs for sampling internals

## Resume Conditions

Resume Pocket TTS integration only after:

- the Engine local vocal loop is repeatable
- the voice/search/runtime boundaries are stable
- route decisions can cleanly select a speech output provider

At that point, the next implementation step is:

1. create a trimmed YAATAL-owned TTS sidecar
2. expose only the narrow synth contract
3. let Engine call it only for the edge/local speech lane
