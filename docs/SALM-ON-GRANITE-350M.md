# SALM-Duplex on Granite 4.0 H 350M — Composition & Build Plan

> Decision record for the sovereign edge-voice stack: how SALM-Duplex works, why Granite 350M Instruct is the aggressive-edge backbone candidate, and what remains to build. Anchors the **SALM** lane of the model program (`EDGE-VOICE-SOVEREIGNTY.md` §Model choices). Snapshot **2026-06-11**.

## TL;DR

SALM-Duplex is not an audio-native model like Liquid. It is a **composition** of three swappable components — encoder, LLM backbone, codec — plus a turn-taking controller. This decouples license from capability.

**Granite 4.0 H 350M Instruct is the new aggressive-edge candidate for the backbone slot.** At 350M parameters (vs. the previously planned 1B), it is small enough to run on mid-range phone SoCs while retaining native function-calling. Apache-2.0 throughout. The open question is whether 350M is large enough for Wolof/French code-mix intent accuracy — a short LoRA smoke resolves this.

**Dual-lane candidate.** The same composition serves both product lanes: in the **Commerce lane** it is the voice agent backbone (intent + tool-call through the text seam), and in the **Livestream lane** it is the duplex engine the Atlantic blueprint calls for (the interactive translation agent). One backbone, one license chain, two products — whether deployed text-only (Nano tool-router) or as full SALM voice.

## 1. What SALM-Duplex actually is

SALM-Duplex (Interspeech 2025, `arXiv:2505.15670`) builds full-duplex speech-to-speech from an **arbitrary LLM backbone**. It does not require an audio-native foundation model.

```text
User speech
  → [Speech encoder]  → continuous text-like embeddings
  → [LLM backbone]    → predicts next text token (one step before speech)
  → [Text seam]       → auditable, validatable, stoppable
  → [LLM backbone]    → predicts next speech token
  → [Neural codec]    → reconstructs audio waveform
  → Assistant speech
```

The LLM backbone is **swappable**. The paper uses a generic decoder; we use Granite 4.0 H 350M Instruct.

## 2. The four components

### Component A — Speech Encoder: Nemotron 3.5 ASR (600M)

| | |
|---|---|
| **Model** | `nvidia/nemotron-3.5-asr-streaming-0.6b` |
| **Size** | 600M parameters |
| **Architecture** | Cache-Aware FastConformer-RNNT |
| **License** | OpenMDW-1.1 (Linux Foundation, no cap, royalty-free) |
| **Status** | Needs Wolof low-resource adaptation |

**What it does:** Converts incoming microphone audio into a stream of text-like embeddings. The RNNT joint network emits label posteriors that the LLM backbone consumes as interleaved tokens.

**Wolof adaptation:** The base model supports ~40 locales, none African. Adaptation uses the NeMo fine-tuning recipe with replay data (French audio replayed as pseudo-Wolof tokens for code-mix stability) and the Greek/Bulgarian low-resource recipe as a phonotactic proxy. Data sources, per `WOLOF-DATA-INVENTORY.md`: **`soynade-research/Wolof-ASR-Data` (97.9h train + 17.8h test, cc-by-sa-4.0)** as the main corpus, `galsenai/wolof-audio-data` (35k rows, Apache-2.0), and `serge-wilson/wolof-french-asr` (47.9k rows, cc-by-4.0) specifically for code-switch robustness.

### Component B — LLM Backbone: Granite 4.0 H 350M Instruct

| | |
|---|---|
| **Model** | `ibm-granite/granite-4.0-h-350m` |
| **Size** | 350M parameters |
| **Architecture** | `GraniteMoeHybridForCausalLM` (32 layers, 768 hidden, bfloat16) |
| **License** | Apache-2.0 |
| **Native capabilities** | Text generation, instruction following, **function-calling** |
| **Languages (base)** | English, German, Spanish, French, Japanese, Portuguese, Arabic, Czech, Italian, Korean, Dutch, Chinese |
| **Wolof support** | None out-of-the-box; requires LoRA fine-tuning on local data |

**What it does in SALM:** Receives interleaved tokens from the encoder. Predicts the next text token **one step before** predicting the next speech token. This ordering is critical — it creates the text seam.

**Why 350M instead of 1B:**
- **~2.5× smaller** than Granite 1B, the qualified SALM backbone (bake-off 2026-06-10: slot_f1 0.797, beat the capped LFM2-Tool, tied the Qwen transformer control)
- Explicitly **function-calling capable** (documented on model card)
- Same `granitehybrid` architecture as Granite 1B — if SALM works on 1B, it ports to 350M with minimal changes
- If intent accuracy holds after LoRA, this becomes the default edge backbone; the 1B stays the quality fallback

**Resolved (was an uncertainty):** `num_local_experts: 0` means exactly what it says — the h-350m and h-1b are **dense** Mamba2+attention hybrids, not MoE. Confirmed by our own GGUF conversion logs for the 1B (`granitehybrid.expert_count = 0`). No dense-fallback mystery; inference cost is the dense cost.

### Component C — Parallel-Codebook Codec (NeMo SpeechLM2)

| | |
|---|---|
| **Source** | NVIDIA NeMo `speechlm2` |
| **Bitrate** | ~0.6 kbps (from SALM paper) |
| **Type** | Parallel RVQ / FSQ codebook |
| **License** | Apache-2.0 (assumed; last unverified link in chain) |

**What it does:** Converts the LLM's discrete speech-token predictions back into audible waveform. This is a neural vocoder, not TTS. It reconstructs speech from a compact latent representation.

**Wolof risk:** The codec was likely trained on English-centric data. Reconstruction quality for Wolof phonemes (implosives, vowel length) is unknown and needs subjective evaluation.

### Component D — Turn-Taking Controller

From NeMo `DuplexS2SModel` and the SALM paper:
- **Barge-in detection:** The encoder remains active while the assistant speaks. User interruptions are detected via acoustic overlap.
- **State machine:** `LISTENING` → `THINKING` → `SPEAKING` → `LISTENING`, gated by confidence thresholds.
- **Endpoint detection:** The RNNT encoder signals when the user has stopped speaking (silence + semantic completion).
- **Backchannel handling:** "Waaw", "Dëgguñu" — the controller must decide whether these are turns or continuations.

## 3. The text seam — how sovereignty is enforced

The most important architectural property of SALM-Duplex is the **text-before-speech prediction order**:

```text
Step 1: Granite predicts text token
        ↓
        TEXT SEAM ← Engine inspects, validates, decides
        ↓
Step 2: Granite predicts speech token
        ↓
        Codec synthesizes audio
```

Because text is predicted first, the Engine can:
1. Read the text token (or accumulated text string) before any audio is generated
2. Extract structured intent JSON from the text
3. Validate against schema, auth, policy, merchant state
4. Decide whether to allow speech continuation or inject a safe fallback

**This means the model cannot bypass the gate.** Speech synthesis is physically downstream of text prediction. The model proposes words; the Engine disposes.

```text
User: "Dama soxla wax Holland bu yomb ci HLM."
  → Encoder → Granite 350M
  → Text seam: "find_product | product=wax Holland | market=HLM | price_constraint=affordable"
  → Engine validation:
      - Schema valid? ✅
      - Auth present? ✅
      - Merchant exists? ✅
      - Policy allows? ✅
  → Allow speech continuation
  → Assistant speaks: "Wax Holland am na ci HLM, sama waay..."
```

If validation fails:
```text
  → Engine rejects
  → Injects fallback text: "Mbind moo am solo..." (safe response)
  → Assistant speaks the fallback instead
```

## 4. Tool-calling in a voice loop

Granite 350M Instruct already has function-calling capability. In SALM-Duplex, this happens **inside the text seam**:

```text
User speech → Encoder → Granite 350M
  → Step 1: text prediction → "find_product | product=wax Holland | market=HLM"
  → Step 2: text seam → tool-call JSON proposed
  → Step 3: speech token prediction → audio response begins
  → Engine gate:
      - Receives: {"intent":"find_product","entities":{...},"needs_tool":true}
      - Validates: schema, auth, merchant state, policy
      - If valid: allows speech + dispatches tool to Engine commerce layer
      - If invalid: interrupts with safe Wolof fallback
  → Codec → Speaker
```

**The model never executes the tool directly.** The audio output is dependent on Engine validation. The tool dispatch is a separate, controlled action.

## 5. Memory and compute budget

| Component | RAM (Q8 GGUF) | RAM (Q4 GGUF) | Notes |
|---|---|---|---|
| Granite 350M backbone | ~420 MB | ~210 MB | 768 hidden × 32 layers |
| Nemotron 600M encoder | ~720 MB | ~360 MB | FastConformer is memory-heavy |
| Parallel-codebook codec | ~100 MB | ~50 MB | Small RVQ decoder |
| **Total resident** | **~1.25 GB** | **~620 MB** | Both fit on 4GB-RAM phones |
| **Inference target** | iPhone 12-class SoC | Comfortable | Realtime depends on encoder latency |

**Comparison with previous candidates:**

| Backbone | Size | Total system (Q4 est.) | Fit |
|---|---|---|---|
| **Granite 350M** | **350M** | **~620 MB** | **Aggressive edge** |
| Granite 1B | 1.46B | ~1.3 GB (measured: 901 MB GGUF + encoder + codec) | Standard edge |
| Liquid LFM2.5-Audio | 1.5B | ~1.3 GB | Edge (capped license) |

(Zamba2 1.2B removed from the ladder: it never ran the qualification; the 2026-06-10 bake-off
settled the backbone family on Granite.)

## 6. What must be built (honest flags)

### A. Wolof speech encoder
The Nemotron 600M base has no Wolof. Required:
- NeMo manifest export from `galsenai/wolof-audio-data`
- 10–20 epoch fine-tune on A10 GPU (Modal or NYIT)
- Held-out WER evaluation on FLEURS Wolof test set
- Target: WER < 30% on code-mix utterances

### B. Code-mix acoustic robustness
Senegalese speech is rarely pure Wolof. The encoder must handle:
- "Dama soxla wax Holland" (Wolof + French loanword + English brand)
- Mid-sentence code-switch
- Market noise, multiple speakers

Requires synthetic code-mix data augmentation and noise injection using recorded Dakar ambient audio.

### C. Codec Wolof quality
The NeMo codec's reconstruction quality for Wolof phonemes is unknown. Requires:
- Subjective listening tests with native speakers
- Objective metrics (MOS, similarity to reference)
- Possibly codec fine-tuning on Wolof TTS output from `galsenai/xTTS-v2-wolof`

### D. Granite 350M LoRA for intent
A short LoRA smoke (60–500 steps) run on the **same Modal harness as the 1B qualification**
(`modal run scripts/modal_bakeoff.py --only granite-4.0-h-350m` after adding it to SPECS) and on
the **6,022-row v2 dataset** (`data_augmented_oolel_v2`, base + 829 Oolel variants), so the numbers
are directly comparable to the champion's scoreboard. "Intent accuracy" maps to the harness
metrics: slot_f1 and exact-match on the held-out split, plus the lexicon probes.

| Outcome (vs the 1B's scores on the same data) | Decision |
|---|---|
| Within ~5% of the 1B on slot_f1 | 350M becomes the default edge backbone |
| 5–15% behind the 1B | 350M for prototype + **distillation catch-up** (1B teacher → 350M student via TRL GKD, on-policy); 1B for production |
| More than 15% behind | Granite 1B stays the backbone; 350M parked for commodity-phone tier via distillation only |

### E. Turn-taking UX
Full-duplex means interruptions are allowed. In Wolof conversational context:
- Overlap tolerance (natural in Wolof speech)
- Backchannel detection ("Waaw", "Dëgguñu")
- Politeness hierarchy (customer vs. merchant vs. elder)

These are interaction-design problems requiring prototype user testing in Dakar, not model research.

### F. Engine voice crate integration
The Engine's `yaatal-voice` is currently a WebSocket mock backend. Building SALM-Duplex requires:
- Replacing mock with real SALM inference loop
- Bridging C-ABI to Rust via `yaatal-voice/build.rs`
- GGUF runtime integration (likely `llama.cpp` bindings or custom inference engine)
- Feature-gating behind `speech-core-sys` (already scaffolded)

Medium engineering effort, not research.

## 7. How this serves the sovereignty thesis

| Property | How SALM-on-Granite-350M satisfies it |
|---|---|
| **Cap-free license** | Apache-2.0 (Granite) + OpenMDW (encoder) + Apache (NeMo) = no revenue threshold, no foreign veto at scale |
| **Text seam audit** | Engine validates every tool-call before speech is synthesized; the model cannot bypass |
| **Edge runnable** | 350M + 600M + codec ≈ 620MB Q4 — fits on commodity phones, not just flagships |
| **Wolof-capable** | Encoder needs adaptation; backbone is language-agnostic via fine-tuning |
| **Tool-calling** | Granite 350M instruct has native function-calling; SALM inherits it in the text seam |
| **Swappable** | Encoder and backbone are modular; if a better Wolof encoder appears, swap it without retraining the backbone |
| **Controllable** | The Engine gate is the authority; the model is the proposer |

## 8. The two-clock strategy

| Clock | Action | Timeline |
|---|---|---|
| **Ship** | Liquid LFM2.5-Audio for near-term BOBO voice experiments (free under $10M) | Now |
| **Build** | SALM-Duplex on Granite 350M as the sovereign replacement | Research track |

**If the 350M LoRA smoke fails:** Fall back to Granite 1B for the SALM backbone (already
qualified), and reach the 350M tier later by distilling the 1B into it (TRL GKD, same tokenizer,
same family) rather than by direct SFT.

## 9. Recommended next action

1. **Run the LoRA smoke** on Granite 350M instruct via the existing Modal harness, on the
   **6,022-row v2 dataset**, scored against the 1B champion's numbers (see §6.D).
2. **If accuracy holds:** Update the edge-intent prototype to use Granite 350M; update
   model-inventory docs (portal + Engine).
3. **Parallel:** Begin Nemotron 600M Wolof encoder fine-tune (NeMo manifest from the 97.9h
   Wolof-ASR-Data corpus → A10 run on Modal or NYIT).
4. **Parallel:** Verify NeMo codec license (last unchecked link in the chain).

## Sources (verified, not memory)

- SALM-Duplex paper — `arXiv:2505.15670` (Interspeech 2025; "from any LLM"; one-token text-before-speech; 0.6 kbps codec)
- NeMo SpeechLM2 — `docs.nvidia.com/nemo/speech/nightly/speechlm2/models.html` (`DuplexS2SModel`, `DuplexSTTModel`)
- Nemotron 3.5 ASR — `huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b`
- OpenMDW-1.1 — `openmdw.ai` (Linux Foundation, "deal without restriction", royalty-free)
- Granite 4.0 H 350M Instruct — `huggingface.co/ibm-granite/granite-4.0-h-350m` (Apache-2.0, function-calling, 12 base languages)
- Granite 4.0 H 350M Base — `huggingface.co/ibm-granite/granite-4.0-h-350m-base`
- Granite 4.0 Nano collection — `huggingface.co/collections/ibm-granite/granite-40-nano-language-models-68e5775c80b60e43b72cfa16`
- Wolof audio data — `huggingface.co/datasets/galsenai/wolof-audio-data`
- Existing edge-intent dataset — `output/yaatal-edge-agent/data/` (5,000 rows, Wolof/French/English)
- Zamba2 1.2B reference — `huggingface.co/Zyphra/Zamba2-1.2B-instruct`
- EDGE-VOICE-SOVEREIGNTY.md — `docs/EDGE-VOICE-SOVEREIGNTY.md`

---

*Document: SALM-ON-GRANITE-350M.md*
*Snapshot: 2026-06-11*
*Status: Decision record — open for revision when LoRA smoke results arrive*
