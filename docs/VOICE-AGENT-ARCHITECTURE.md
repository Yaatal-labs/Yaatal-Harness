# Voice agent — inventory, holes, and where it plugs in

**Recorded in Harness** because Harness is the AI control plane and the Engine
was not writable when this was found. Mirror it into the Engine when it is.

Everything below was verified by reading source on 2026-08-23. Engine at
`2d1a06d`, `soniqo/speech-core` at `ff1c3e2`. Nothing was modified.

This exists because the voice story is spread across four repos and one C++
submodule, and reading any one of them alone gives a wrong answer. It gave me
two.

---

## 1. Inventory — what is actually built

### Engine

| Component | What it is | State |
|---|---|---|
| `controllers/livekit.rs` | WebRTC transport: JWT token mint + signed webhook. Identity forced from `auth.claims.pid`. `one_to_one` only, others 501 | **shipped** — routed, configured dev+prod, env `LIVEKIT_{URL,API_KEY,API_SECRET}`, inventory-tested |
| `yaatal-voice/session.rs` | Safe Rust facade over the speech-core C ABI | **real, no caller** — nothing constructs `SessionBuilder` |
| `yaatal-voice/sys.rs` | bindgen FFI + a mock for `--no-default-features` | real |
| `services/voice_routing.rs` | Edge/cloud lane decision, sticky state, repair detection | **real, fed constants** |
| `services/voice_session.rs` | WS session driving the above | real |
| `controllers/voice.rs` | `POST /api/voice/transcribe` → HF whisper-large-v3 | **wired — and the leak** |
| `yaatal-core/src/ai/**` | the 5-tier text cascade | text only; no audio, no image |

### speech-core (`third_party/speech-core`, submodule — **0 files checked out**)

C++17, on-device, CPU. Its own README: *"No cloud, no Python at inference, and
no audio leaves the machine."* That single sentence is why it is the right
foundation for a sovereign voice lane.

**Everything is a vtable — nothing is hardcoded:**

```c
sc_pipeline_t sc_pipeline_create(
    sc_stt_vtable_t stt,      // caller-supplied
    sc_tts_vtable_t tts,      // caller-supplied
    sc_llm_vtable_t* llm,     // NULL for Echo / TranscribeOnly
    sc_vad_vtable_t vad,      // caller-supplied
    sc_config_t config, sc_event_fn on_event, void* ctx);
```

Plus `sc_enhancer_vtable_t` (denoise) and `sc_echo_canceller_vtable_t`
(`feed_reference` / `cancel_echo` — what makes barge-in work *during* playback).

Silero / Parakeet / Kokoro / DeepFilterNet3 are **reference implementations**
behind the optional `models` feature, not the pipeline. Yaatal's own organs plug
in as `sc_stt_vtable_t` / `sc_tts_vtable_t`.

Barge-in is tunable, not just an event:
`allow_interruptions`, `min_interruption_duration` (filters AEC residual echo),
`interruption_recovery_timeout` (false-interruption recovery),
`post_playback_guard`.

It also carries its own tool calling (`sc_tool_definition_t` — a callback
`handler`, **or a shell `command`**), conversation trimming, and eager/warmup
STT.

### Harness

| Component | State |
|---|---|
| `yaatal-edge-turn` | **complete and running.** Transcript → local proposal → validate against a closed `ToolName` enum → Decision + audit. **Hard-requires loopback** (`MINIMIND_URL must target a loopback host`) |
| `yaatal-voice` (Harness's own) | trait skeleton — `SpeechToText`/`IntentParser`/`TextToSpeech`, **mock impls only** |
| `yaatal-pi-bridge` | custody core + Node planner + cascade provider. **No transport, no caller** |

### Studio

`live/agent_loop/stt_listener.py` — `inject_text()` works; both real backends
(`_transcribe_via_voicebox`, `_transcribe_via_faster_whisper`) raise
`NotImplementedError`. `live/harness_client.py` and
`live/test_edge_turn_e2e.py` mean the Studio → Harness edge-turn wire exists and
is tested; only the microphone end is missing.

---

## 2. The holes

**H1 — `/api/voice/transcribe` ships raw audio to HuggingFace.**
`controllers/voice.rs:27` calls `TranscriptionRouter::transcribe(&body, false)`.
No classification, no approved-target check, no way to request the edge. The
Cargo feature `legacy-cpal-whisper` already describes this path as legacy
("*for emergency revert*"), so it is a stopgap that outlived its replacement.

**H2 — the voice privacy control is wired to a constant.**
`voice_session.rs:111-124` hardcodes `privacy_sensitive: false` and
`network_available: true`, so **both** force-edge paths in `voice_routing.rs`
are unreachable. The control is correct and tested; nothing feeds it. For a lane
premised on African infrastructure reality, asserting the network is always up
is the worse of the two.

**H3 — the Rust facade exposes 4 of ~22 speech-core config fields.**
`SessionBuilder` offers `sample_rate`, `channels`, `vad_threshold`, `language`.
No `sc_mode_t`, no vtables, no barge-in tuning, no tool definitions. This is why
speech-core *looked* hardcoded. Nothing in the core forces it.

**H4 — speech-core has no caller.** A complete on-device pipeline that nothing
constructs, while H1 ships audio to the cloud.

**H5 — nothing wires LiveKit audio into a speech-core session.** LiveKit moves
audio between people; speech-core processes it on one machine. Both are real;
the wire between them does not exist. (speech-core mentions LiveKit only in a
comment citing its 0.5 s default.)

**H6 — four places call a model.** The cascade (`Gateway::infer`), edge-turn's
`ProposalBackend`, Pi's `Provider`, and speech-core's `sc_llm_vtable_t`. The
"one trait, two lanes" decision has to account for the fourth.

**H7 — `sc_tool_definition_t.command` is a shell escape hatch** inside the voice
pipeline. Never use it; only `handler`, so Rust owns execution.

---

## 3. Where the voice agent plugs in

**Ownership, stated once:** every audio-handling component is Engine-side —
LiveKit transport, the speech-core facade and submodule, voice routing, voice
session, and the transcribe endpoint. Harness owns no audio at all: `edge-turn`
takes a `Transcript`, already text. Studio owns only the consumer end. So
V1-V6 are Engine changes, V7 is Studio + Harness, and V8 is cross-cutting.


```
                    ┌──────────── people ────────────┐
   buyer/seller ──► LiveKit room ──► audio ──┐
   (WebRTC, Engine mints the token)          │
                                             ▼
                          ┌──────── speech-core session ────────┐
                          │  VAD ─► STT ─► [barge-in / AEC]     │
                          │   ▲              │                  │
                          │   │              ▼                  │
                          │  TTS ◄──── transcript               │
                          └──────────────────┼──────────────────┘
                                             │  Transcript
                                             ▼
                              ┌──── Harness: edge-turn ────┐
                              │ validate → policy → audit  │
                              │ closed ToolName enum       │
                              └──────────────┼─────────────┘
                                             ▼
                                     Studio / Engine action
```

**The rule that decides the shape:** `sc_llm_vtable_t` is left `NULL` and the
mode is `SC_MODE_TRANSCRIBE_ONLY`. speech-core does ears, mouth, and turn-taking
— it does **not** do the thinking. The decision goes to edge-turn, where the
closed tool enum, the policy gate, and the audit trail already live.

That keeps one custody boundary instead of two, and it means speech-core's own
tool calling (H7) is never used.

**Which agent serves which role:**

| Role | Input | Loop | Model lane |
|---|---|---|---|
| `studio-live` | seller speech | speech-core → edge-turn | **LoopbackLocal** (structural) |
| `ops-runner` | text | pi-bridge | **Cascade** |
| `merchant-agent` | text | pi-bridge | Cascade |
| `dev-agent` | text | pi-bridge | Cascade |

The lane is a property of the role's data class, not of which crate it lives in.
Seller speech is Sovereign, so it takes the lane whose guarantee is structural —
edge-turn already refuses a non-loopback model, and speech-core lets no audio
leave the machine. Neither depends on a classifier being right about Wolof.

---

## 4. Execution scope

Ordered by risk removed per unit of work. Each leaves the workspace compiling
and lands independently.

### V1 — Close the transcribe leak *(Engine, small)*
Gate the HF path behind explicit approval and refuse when unset, rather than
silently shipping audio. A refusal is a smaller diff than a classifier and
removes the leak today; `legacy-cpal-whisper` already frames this path as
revert-only.
**Check:** approval unset → error, and **no outbound request** (mirror the
`gateway.rs` wiremock test asserting `server.received_requests()` is empty).

### V2 — Feed the two safety signals *(Engine, small)*
`privacy_sensitive` from `classify_data_class` (reuse the one classifier),
`network_available` from the existing `NetworkGate`.
**Check:** a transcript with a phone number routes Edge with reason
`privacy_sensitive`; an Offline condition routes Edge with
`network_unavailable`.

### V3 — Widen the speech-core facade *(Engine, medium)*
Expose `sc_mode_t`, the vtable slots, and the barge-in fields on
`SessionBuilder`. **This is the real first task** — wiring a caller to a 4-knob
builder would hard-code the very choices that must stay open. Vendor or fetch
the submodule as part of this.
**Check:** a `TRANSCRIBE_ONLY` session with `llm = NULL` and a stub STT vtable
emits `SpeechStart → Partial → Final` from fed PCM.

### V4 — Yaatal organs as vtables *(edge lane, medium)*
Wrap the bake-off winner as `sc_stt_vtable_t` and the Wolof TTS as
`sc_tts_vtable_t`. Keep Silero VAD — it is language-agnostic and already tuned.
**Check:** a Wolof clip through the real vtables produces a transcript the
`asr_roundtrip` criterion accepts.

### V5 — Retire the HF path *(Engine, small)*
Once V3+V4 land, delete `transcribe.rs`'s cloud branch rather than leaving two
STT routes. Deletion over addition.

### V6 — LiveKit → speech-core *(Engine, medium)*
Pipe room audio into a session. This is H5 and the last wire before a voice
agent can sit in a call. **Both ends are Engine-side** — `controllers/livekit.rs`
and `yaatal-voice` are in the same crate tree — so this is one repo's change,
not a cross-repo negotiation. Studio consumes the result; it does not own the
wire.
**Check:** audio published to a room produces `Final` transcripts server-side.

### V7 — `studio-live` on edge-turn *(Harness, small)*
Studio replaces `stt_listener.inject_text` with real `Final` events. The
Studio → Harness wire and its e2e test already exist.
**Check:** `live/test_edge_turn_e2e.py` passes driven by speech, not injection.

### V8 — One model-lane trait *(cross-cutting, medium)*
Unify Pi's `Provider`, edge-turn's `ProposalBackend`, and speech-core's
`sc_llm_vtable_t` behind one trait with `Cascade` and `LoopbackLocal` lanes.
Do this **after** V1-V7: the lanes are easier to name once all four call sites
are known and one of them (`sc_llm_vtable_t`) is deliberately unused.

---

## 5. How this reorients the Pi plan

**Unchanged.** The custody core, the weighted verdict, the cascade provider, and
`ops-runner` as the first role. Slice 4 (the Rust↔Node transport) is still the
next Pi task and is unaffected — ops planning is genuinely text.

**Corrected.** Phase 4 listed `studio-live` as a Pi role and claimed a role is
"actor id + tool allowlist + system prompt + model choice." It is not, for this
role: `studio-live` belongs on **edge-turn**, whose closed `ToolName` enum and
typed domain limits (`MAX_PRICE_FCFA`) are guarantees a manifest entry cannot
express — in a path that changes displayed prices. A role also needs a fifth
field, its **model lane**.

**Added.** Phase 5 (container isolation) was the only deployment concern named.
The voice lane adds one: speech-core needs cmake, ONNX runtime, and the
submodule in the build image, and the `models` feature must stay **off** once
Yaatal organs replace the reference set.

**Reordered.** The plan treated voice as Phase 4, after the transport. V1 and V2
are live sovereignty gaps and do not depend on any Pi work. They come first.
