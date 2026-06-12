# SALM-Duplex on Granite 4.0 H 350M — Composition & Build Plan

> Decision record for the sovereign edge-voice stack: how SALM-Duplex works, why Granite 350M Instruct is the aggressive-edge backbone candidate, and what remains to build. Anchors the **SALM** lane of the model program (`EDGE-VOICE-SOVEREIGNTY.md` §Model choices). Snapshot **2026-06-11**.

## TL;DR

SALM-Duplex is not an audio-native model like Liquid. It is a **composition** of three swappable components — encoder, LLM backbone, codec — plus a turn-taking controller. This decouples license from capability.

**Granite 4.0 H 350M Instruct is the default edge backbone — gate passed 2026-06-12.** The LoRA
smoke ran on the same Modal harness and the same v2 dataset as the 1B: slot_f1 0.846 vs the 1B's
0.879 (a 3.8% gap, inside the ≤5% gate), intent accuracy **0.993 vs 0.960** (the 350M wins), JSON
validity 1.0 for both, and a ~210 MB Q4 GGUF vs 901 MB. Known weakness: exact-match (0.073 vs
0.353) — full-dict perfection is where the 1B's capacity shows; the distillation catch-up
(1B teacher → 350M student) is the planned closer. Apache-2.0 throughout.

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

**Apache fallback (added 2026-06-12):** [`OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano`](https://hf.co/OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano)
— 22M-param neural codec, **license: Apache-2.0 (verified on the model card)**, 100K+ downloads,
official ONNX build. If the NeMo codec license check fails or its Wolof reconstruction is poor,
this is the drop-in candidate for the codec slot. Compatibility with the SALM parallel-codebook
interface needs a spike (different token layout is the main risk). Sibling model
[`MOSS-TTS-Nano-100M`](https://hf.co/OpenMOSS-Team/MOSS-TTS-Nano-100M) (100M-param CPU-capable TTS,
Apache-2.0, fr/ar among 20 langs) is the new lead candidate for the **Boplex TTS mouth lane**:
official SFT recipe at [OpenMOSS/MOSS-TTS-Nano `finetuning/`](https://github.com/OpenMOSS/MOSS-TTS-Nano)
(plain `audio`+`text` JSONL, ~3.2 GiB VRAM, full SFT), with a **West African precedent** —
`ghananlpcommunity/moss-tts-nano-twi-sft` (Twi, 2026-06-04) — and a Norwegian new-language LoRA in
`community/`. Rust inference exists (`ramishi/moss-tts-nano-candle`), relevant for Engine integration.

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

**Status: Pipeline scripts written. GPU execution pending.**

The Nemotron 600M base has no Wolof. The NeMo manifest pipeline is complete:

| Artifact | File | Status |
|---|---|---|
| Manifest converter | `scripts/nemo_asr_00_create_manifest.py` | ✅ Written & dry-run verified (10 samples) |
| Fine-tune boilerplate | `scripts/nemo_asr_01_finetune.py` | ✅ Written (533 lines, RNNT + FastEmit + SpecAugment + AdamW/cosine + LoRA toggle) |
| Modal runner | `scripts/nemo_asr_01_finetune_modal.py` | ✅ Working `modal run` script (CPU manifest phase + A10G GPU fine-tune phase) |

**Dataset:** `galsenai/wolof-audio-data` — 35,075 samples, ~68 hours, Apache-2.0.

**Dataset upgrade (2026-06-12):** [`soynade-research/Wolof-ASR-Data`](https://hf.co/datasets/soynade-research/Wolof-ASR-Data)
— the Oolel team's curated **116 h** (97.9 train / 17.8 test: FLEURS + ALFFA + CommonVoice + Kallama
+ UB), CC-BY-SA-4.0. Supersedes the single-source manifest as the encoder fine-tune base; extend
`nemo_asr_00_create_manifest.py` to ingest it. Companion asset:
[`soynade-research/Wolof-Non-Standard-Orthography`](https://hf.co/datasets/soynade-research/Wolof-Non-Standard-Orthography)
(informal→standard text pairs) — use it (a) as the transcript normalizer when pseudo-labeling
in-the-wild audio, (b) to harden router training data against real-world spelling, and (c) to
normalize ref/hyp in CER-based evals so orthography drift doesn't read as model error.

**Planned in-domain lane:** YouTube micro-trottoir / code-switched public events (pipeline seed:
`scripts/yaatal_df_08_extract_youtube_mapping.py`): yt-dlp → VAD segment → pseudo-label with best
Wolof ASR → orthography-normalize → confidence filter → human spot-review (Supabase→Sheets→n8n) →
fine-tune mix. **Ears only** — spontaneous/noisy audio degrades TTS voices; the transcripts (not the
audio) feed the mouth lane's text normalizer. Research-use posture: keep provenance, never
redistribute audio. Also on HF already: [`serge-wilson/wolof-french-asr`](https://hf.co/datasets/serge-wilson/wolof-french-asr)
(unified wo-fr, tagged code-switching, CC-BY-4.0).

**Cost:** ~$3–4 for a 10-epoch fine-tune on Modal A10G (fits inside $30 starter credit).

**Remaining:** GPU execution only. Requires Modal token + HF token in environment.

**Fine-tune targets:**
- 10–20 epochs on A10 GPU (Modal or NYIT)
- Held-out WER evaluation on FLEURS Wolof test set
- Target: WER &lt; 30% on code-mix utterances

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
- Fallback if NeMo codec fails on license or Wolof quality: MOSS-Audio-Tokenizer-Nano (Apache-2.0, §2.C)

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

**RESULT (2026-06-12, 60-step LoRA, 150-row v2 held-out):**

| Metric | 350M | 1B | Read |
|---|---|---|---|
| slot_f1 | 0.846 | 0.879 | **3.8% gap → GATE 1 PASSED** |
| intent accuracy | **0.993** | 0.960 | 350M wins |
| exact_match | 0.073 | 0.353 | 1B's capacity edge; distillation target |
| JSON validity | 1.000 | 1.000 | tie |
| Q4_K_M GGUF | ~210 MB | 901 MB | both exported, in `yaatal-bakeoff-out` |

**→ Granite 350M is the default edge backbone.** The 1B remains the quality fallback and the
designated distillation teacher for closing the exact-match gap. Caveats: synthetic data
(needs_native_review), 60-step smoke, 150-row eval — the human-reviewed eval set replaces this
scoreboard as it lands.

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

## 9. Recommended next actions

### Immediate (no GPU needed)
1. ✅ **Manifest converter verified** — `scripts/nemo_asr_00_create_manifest.py` dry-run passed.
2. ✅ **Fine-tune boilerplate written** — `scripts/nemo_asr_01_finetune.py` ready.
3. ✅ **Modal runner written** — `scripts/nemo_asr_01_finetune_modal.py` ready for `modal run`.
4. ✅ **Cost analysis documented** — `docs/COST-AND-CODEC-ANALYSIS.md` (240 lines).

### Requires GPU / credentials
5. **Run the LoRA smoke** on Granite 350M instruct via the existing Modal harness, on the
   **6,022-row v2 dataset**, scored against the 1B champion's numbers (see §6.D).
6. **Run Nemotron ASR fine-tune** via Modal: `modal run scripts/nemo_asr_01_finetune_modal.py --epochs 10`.
   Estimated time: ~3 hours. Estimated cost: $3–4.
7. **Verify NeMo codec license** — last unchecked link in the sovereign chain.

### Decision gates
| If LoRA smoke result | Then |
|---|---|
| Within ~5% of 1B on slot_f1 | 350M becomes default edge backbone |
| 5–15% behind | 350M for prototype + distillation catch-up; 1B for production |
| More than 15% behind | 1B stays backbone; 350M parked for commodity-phone tier via distillation only |

### Parallel tracks
- **Encoder:** Begin Nemotron 600M Wolof fine-tune (Modal A10 or NYIT GPU).
- **Codec:** Verify license, assess Wolof reconstruction quality, plan TTS-data fine-tune if needed.
- **Engine:** Begin `yaatal-voice` crate integration planning (Python sidecar → HF export → Rust ML runtime).

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
*Snapshot: 2026-06-12*
*Status: Decision record — LoRA smoke complete, gate passed: 350M is the default edge backbone*
