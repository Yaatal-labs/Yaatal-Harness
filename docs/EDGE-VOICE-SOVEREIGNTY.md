# Edge Voice — Sovereignty & License Strategy (Decision Record)

> **Why the edge voice + agentic model is a SALM-Duplex composition on a clean-license hybrid
> backbone — not Liquid — and how to keep options.** Anchors the **Edge voice** lane of the model
> program (`STATE.md` §Model choices · `ENGINE-MANIFEST` §05). Snapshot **2026-06-08**.
> The code/lineup wins if this drifts — update in the same change.

## TL;DR — the thing no session should lose

1. **Liquid (`LFM2.5-Audio`) is the best edge-voice tech** (audio-native, hybrid, efficient) — **but
   the LFM Open License v1.0 has a $10M annual-revenue threshold.** Above it you renegotiate with a
   US lab → a foreign dependency *at scale*, which contradicts the sovereignty thesis.
2. **SALM-Duplex decouples license from capability.** Its claim is *"duplex S2S from **any** LLM"*
   + tool-calling. So you don't *adopt* a capped audio model — you **pick a clean-license hybrid LLM
   backbone and add full-duplex voice + tool-call to it.**
3. **Answer (cap-free sovereign edge voice): SALM-Duplex on `granite-4.0-h-1b`.** It's the only
   candidate that is **Apache-2.0 (no cap, no foreign dependency) AND hybrid-edge-efficient AND has
   native function-calling.** SALM *inherits* Granite's tool-calling — that's why "the SALM way
   brings tool-call." The whole license chain is permissive/no-cap.
4. **Keep options (two clocks):** ship on **Liquid now** if useful (free under $10M, swappable behind
   the Engine's `propose→dispose` seam); **build SALM-on-Granite** as the sovereign replacement on the
   research clock. **Moshi** (CC-BY-4.0) is the clean-license full-duplex *reference/fallback*.

## The SALM license chain — all no-cap (verified where checkable)

| Component | License | Cap? | Status |
|---|---|---|---|
| NeMo SpeechLM2 framework | Apache-2.0 | none | — |
| Encoder — Nemotron 3.5 ASR | **OpenMDW-1.1** | **none** | ✅ verified at openmdw.ai (LF, "deal without restriction", royalty-free) |
| LLM backbone — *your pick* | **Granite-4-h = Apache-2.0** (or Nemotron-H = NVIDIA-Open) | **none** | ✅ Apache confirmed on HF |
| Parallel-codebook codec (NeMo) | (NeMo / Apache, assumed) | ? | ⚠️ **verify — last unchecked link** |
| Your trained duplex weights | yours (you train it) | none | — |

## Option space for the edge voice + agentic slot (license-first)

| Option | Voice | Tool-call | Edge size | License | Cap | Read |
|---|---|---|---|---|---|---|
| **Liquid `LFM2.5-Audio`** | audio-native | claimed *(unverified)* | 1.5B hybrid ✓ | LFM Open v1.0 | **$10M rev** | best tech, foreign cap at scale |
| **SALM-Duplex on `granite-4-h`** | full-duplex (built) | **yes — Granite native fn-calling** | 1.5B backbone + 0.6B enc ✓ | Apache + OpenMDW + Apache | **none** ✅ | **the cap-free sovereign answer — but you *build* it** |
| SALM on Nemotron-H | full-duplex | yes | backbone ≥9B ✗ | NVIDIA-Open + OpenMDW | none | clean but not edge-sized |
| Moshi (Kyutai) | full-duplex native | weak (`<ret>`, not `<tool>`) | 7B ✗ | **CC-BY-4.0** | none | cleanest single license, big + English |
| Qwen2.5-Omni | streaming | yes | 5.5B ✗ | "other"/research | restricted | not clean, not tiny |

## Why this serves the vision

The Engine's primitive is **"models propose, the Engine disposes"** with **sovereignty as a type, not a
setting**. A revenue-capped foreign license on the *edge brain itself* is a sovereignty hole the type
system can't close — at scale it becomes a foreign veto. **SALM-on-a-clean-backbone removes that hole:**
the edge proposer is assembled entirely from permissive, no-cap, swappable parts, with the auditable
**text seam** (SALM predicts text one token before speech — `arXiv:2505.15670`) preserved for the
gov/public-good lane. This is "what hyperscalers won't build": sovereign by construction, all the way
down to the on-device voice model's license.

## VERDICT (2026-06-10) — granite qualified ✅

The qualification run completed (Modal L4s, 60-step LoRA on the 185-row synthetic bootstrap,
23-row val): **granite-4.0-h-1b (clean) beat the capped Liquid ceiling on every capability axis**
— slot_f1 **0.797 vs 0.642**, exact 0.304 vs 0.043, probes 3/5 vs 2/5 — and tied the same-size
transformer control (qwen slot_f1 0.793). LFM2's only wins were size (731 vs 901 MB Q4_K_M) and
CPU speed (~+20%), i.e. its 1.2B-vs-1.46B param count, not capability. Granite's GGUF exported
cleanly incl. SSM tensors. Caveats: tiny synthetic val set; LFM2 exact-match may be depressed by
tool-format mismatch (slot_f1 is the robust signal); bench on few-vCPU containers.
**Decision: `granite-4.0-h-1b` is the SALM backbone.** Full numbers:
`output/yaatal-data-factory/reports/bakeoff_summary.json`.

## The bake-off's real job

Not "pick a text router" — **qualify the clean-license SALM backbone.** The tool-call test on Granite
*is* the SALM-backbone qualification (does a cap-free Apache hybrid do the agentic *propose* job well
enough to wrap in SALM and replace Liquid?). Decision-relevant run:
**`granite-4-h` (clean candidate) vs `LFM2-1.2B-Tool` (the capped Liquid *ceiling* to match) vs
`Qwen2.5-1.5B` (transformer control).** Scripts: `scripts/modal_bakeoff.py` (+ `train_tool_router_hf.py`,
`eval_tool_router.py`, `export_gguf.py`). Output GGUF = candidate fill for the cascade **Tier-1**
placeholder (`ENGINE-MANIFEST` §4.8 / E7-A).

## Honest flags / open verifications

- **SALM-on-Granite is BUILD, not adopt** — a real duplex retrain (Doc 1: "NYIT-scale, not a $3 fine-tune").
- **Wolof encoder bootstrap** — Nemotron-3.5-ASR's ~40 locales do **not** include Wolof; the encoder
  needs low-resource adaptation (Greek/Bulgarian recipe; replay French for code-mix).
- **NeMo codec license** — the one unverified link in the chain above.
- **Liquid's tool-call/strict-JSON claim** is itself flagged *unverified* — if it fails, Liquid's edge
  advantage shrinks regardless of license.

## Sources (verified, not memory)

- OpenMDW — openmdw.ai (Linux Foundation, permissive, no cap)
- LFM Open License v1.0 $10M threshold — YAATAL model-inventory doc
- SALM-Duplex — `arXiv:2505.15670` (Interspeech 2025; "from any LLM"; one-token text-before-speech; 0.6 kbps codec)
- NeMo `speechlm2` classes — docs.nvidia.com/nemo/speech/nightly/speechlm2/models.html (`DuplexS2SModel`, `DuplexSTTModel`, …)
- `granite-4.0-h-1b` Apache-2.0 + native fn-calling — HF `ibm-granite/granite-4.0-h-1b` / `-micro`
- Lineup of record — `STATE.md` §Model choices · `ENGINE-MANIFEST` §05
