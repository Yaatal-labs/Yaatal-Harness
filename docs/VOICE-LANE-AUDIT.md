# Voice lane — sovereignty audit and fix plan

**Findings about Yaatal-Engine, recorded in Harness** because Harness is the AI
control plane (runtime custody, audit, behavioural policy) and the Engine was
not writable when this was found. Move or mirror it into the Engine when it is.

Audited 2026-08-23 against Yaatal-Engine `2d1a06d`. Read-only; nothing changed.

## What is actually there

Substantial voice work exists — ~3,300 lines:

| Component | Lines |
|---|---|
| `crates/yaatal-voice/src/**` | 2,005 |
| `crates/yaatal-api/src/services/voice_routing.rs` | 497 |
| `crates/yaatal-api/src/services/voice_session.rs` | 384 |
| `crates/yaatal-api/src/controllers/livekit.rs` | 350 |
| `crates/yaatal-api/src/controllers/voice.rs` | 74 |

**It has its own data-protection design, and a good one.** It does not reuse the
text pipeline's `Sensitivity` / `route_declared` vocabulary, which is why a grep
for those terms comes back empty and reads — wrongly — as "voice has no
sovereignty story." It has one, in its own words:
`VoiceTurnSignals.privacy_sensitive` → `force_edge` → `VoiceRouteLane::Edge`
(`voice_routing.rs:176-178`). Privacy-sensitive audio is forced to the edge lane
and never reaches cloud. The logic is correct and tested.

## The two real gaps

### 1. The privacy control is wired to a constant

`voice_session.rs:111-124` builds `VoiceTurnSignals` with five signals
hardcoded:

```rust
asr_confidence: None,
intent_confidence: None,
entity_count: 0,
tool_candidate_count: 0,
duplex_requested: false,
network_available: true,     // force-edge-on-no-network never fires
privacy_sensitive: false,    // force-edge-on-privacy never fires
```

So the router is a well-built decision engine being fed constants. In production
it can only ever see `transcript`, `duration` and `audio_chunk_count`. **Both
force-edge paths are unreachable** — the privacy one and the offline one. The
control is not missing, it is disconnected.

`network_available: true` is the more embarrassing of the two: the lane whose
entire premise is African infrastructure reality currently asserts the network
is always up.

### 2. `POST /api/voice/transcribe` has no lane decision at all

`controllers/voice.rs:26-27` calls
`TranscriptionRouter::transcribe(&body, false)` — the `false` is `offline`. That
reaches `api-inference.huggingface.co` with `openai/whisper-large-v3`
(`transcribe.rs:5,45,95,104`).

This path never touches `VoiceRoutingSession`, so it has no privacy signal to
ignore. Raw customer audio posted to that endpoint goes to HuggingFace
unconditionally. Recorded speech is about as Sovereign as data gets, and there
is no classification, no approved-target check, and no way for a caller to ask
for the edge.

The local alternative is a stub: `transcribe.rs:80` logs "Local candle inference
is not supported on this build target" and `:81` is `TODO(#E7)`.

## Fix plan

Ordered by ratio of risk removed to work. None of this is Pi work; it stands on
its own.

### F1 — Close the transcribe bypass (do first)
`POST /api/voice/transcribe` must not reach a remote processor for audio that
has not been classified. Cheapest correct move is to refuse rather than to
build: gate the cloud path behind an explicit approval env (the shape already
exists as `BYO_SOVEREIGN_APPROVED` in `AiConfig`) and return a clear error when
it is unset, instead of silently shipping audio to HF. A refusal is a smaller
diff than a classifier and removes the leak today.
**Check:** a request with approval unset returns an error and makes no outbound
request — mirror the existing `gateway.rs` wiremock test that asserts
`server.received_requests()` is empty.

### F2 — Connect `privacy_sensitive`
Thread a real value into `routing_signals()`. The Engine already classifies
text: `classify_data_class` in `yaatal-core/src/ai/classify.rs` is what
`route_declared` uses. Running it over the turn's transcript and mapping
`Sensitivity::Sovereign` to `privacy_sensitive: true` reuses the one classifier
rather than adding a second — the rule the Engine's CLAUDE.md already states.
**Check:** a transcript carrying a phone number routes Edge with reason
`privacy_sensitive`.

### F3 — Connect `network_available`
`NetworkGate` already exists (`yaatal-core/src/ai/network.rs`) and the text
cascade uses it. Feed its condition in rather than `true`.
**Check:** an Offline condition forces Edge with reason `network_unavailable`.

### F4 — The remaining stubs
`asr_confidence`, `intent_confidence`, `entity_count`, `tool_candidate_count`,
`duplex_requested` are placeholders. They degrade routing *quality*, not safety,
so they rank below F1-F3. Worth a `ponytail:` comment naming them as known-inert
so the next reader does not assume the router is fully informed.

## What this means for the Pi bridge

Little, directly — but it corrects a claim made while planning it.

The 5-tier cascade **is** text-only (`Message { role, content: String }`; no
audio or image anywhere in `yaatal-core/src/ai/`). Voice does not flow through
`Gateway::infer`, and that is by design, not omission: the voice lane is a
separate pipeline with its own edge/cloud router.

So a voice-driven agentic role (`studio-live`) cannot be built on
`/api/ai/chat`. It needs either the voice lane to grow a tool-calling seam, or
the cascade to grow a multimodal message type. That is an Engine decision and it
should be made deliberately, not discovered mid-slice.

`ops-runner` — the only role the bridge serves today — plans CLI invocations and
is genuinely text, so slice 4 is unaffected.
