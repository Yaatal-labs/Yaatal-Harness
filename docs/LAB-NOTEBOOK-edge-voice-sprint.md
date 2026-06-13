# Lab notebook: the edge-voice sprint

*Internal documentation. June 12, 2026. Covers the brain gate, five mouth cycles,
the ears debugging ladder, and the ears gate (in flight as of writing). One day,
~$24, three model families touched.*

The goal, stated at the start of the day: a full-duplex Wolof voice assistant
that fits on an ordinary phone, built from swappable open components, with a
demo page on the Registry. The fallback goal: at least one finished training
run loaded in Modal. Both were met by mid-afternoon; the rest of the day went
to making the results better and the record durable.

Guiding line, as always: the main barrier is Access and Reach, not Creativity,
Vision or Talent.

## 1. Brain: the 350M gate

Question: can Granite 4.0-H 350M replace the qualified 1B as the edge router?
Method: same Modal harness, same 6,022-row v2 dataset, same 150-row held-out
split for both models. Gate: within 5% of the 1B on slot F1.

| Metric | 350M | 1B | Read |
|---|---|---|---|
| Slot F1 | 0.846 | 0.879 | 3.8% gap, inside the gate |
| Intent accuracy | 0.993 | 0.960 | the smaller model wins |
| JSON validity | 1.000 | 1.000 | tie |
| Exact match | 0.073 | 0.353 | the 1B's capacity edge |
| Q4 GGUF | 210 MB | 901 MB | 4.3x |

Verdict: 350M is the default edge backbone. The 1B stays as quality fallback
and distillation teacher (the exact-match gap is the distillation target).
Both checkpoints publish so future work can build on either.

Lesson: at this task shape (utterance to structured intent), intent accuracy
saturates before capacity does. Exact match is where capacity shows, and the
propose-validate contract makes exact match the least important metric.

## 2. Mouth: five autoresearch cycles on MOSS-TTS-Nano

Morning discovery: OpenMOSS released MOSS-TTS-Nano-100M in April (Apache-2.0,
CPU-capable, official fine-tuning recipe, ~3.2 GiB VRAM). GhanaNLP fine-tuned
it for Twi on June 4. That precedent plus the recipe made the Wolof attempt a
same-day project. To our knowledge these became the first Wolof checkpoints of
this model family.

The loop: each cycle trains on Modal (~$3), then evaluates 20 sentences
(10 fixed validation, 10 rotating; half clean held-out Wolof, half Boplex
market turns in wo-fr code-mix) against six binary criteria, including an ASR
round-trip CER judge (asr-africa w2v-BERT, 75h Wolof) and a duplex
end-to-end check that chains the 350M router to the new voice.

| Run | Mutation | Val CER | Verdict | What it taught |
|---|---|---|---|---|
| 1 | baseline: 3k clips, 2 epochs | 0.767 | KEEP (baseline) | it speaks; words do not hold |
| 2 | 2x data, 2x epochs | 0.718 | DISCARD | scale alone is not the constraint |
| 3 | voice-clone eval (same ckpt) | 0.729 | DISCARD | conditioning fixed stability (20/20 sanity), not lexicon |
| 4 | lr 3e-5, 6 epochs | 0.731 | KEEP (tiebreaker) | short sentences land ("Yaa ngi ci xet wi" at CER 0.12); optimization was half the wall |
| 5 | + women_wolof_tts, 12k clips | 0.744 | DISCARD | native register broadened, code-mix degraded: the added data has zero French |

The founder's listening verdict on run 3 ("picking and dangling: Wolof words
here and there, not pure Wolof") redirected the loop from hyperparameters to
data, and run 4/5 confirmed both halves of that diagnosis.

Lessons to keep:

- Phonology forms before lexicon. The model sounded like Wolof (STOI 0.95)
  long before it said Wolof words. Judge accordingly: an audio-quality metric
  alone would have called run 1 a success.
- The eval must match the deployment register. Clean read-speech training
  moved clean-sentence CER and did nothing for market code-mix. The next data
  mutation is a French/code-mix blend, which independently matches NVIDIA's
  fine-tuning guidance for Nemotron (blend base languages).
- A human ear at the right moment beats three compute cycles. The run-3 listen
  cost nothing and set the direction for everything after.
- Voice-clone conditioning at inference is free quality for a new language:
  it eliminated empty/degenerate outputs entirely.

## 3. Ears: the debugging ladder and the gate

The NeMo fine-tune scripts existed (written by the review conveyor, dry-run
only). Getting them to actually train took eleven failures. The ladder, in
order, each one committed so it never repeats:

1. torchcodec missing for datasets audio decode (mouth lane) - pin datasets
2. sentencepiece missing for the MOSS tokenizer - add package
3. torch.load CVE gate on .bin checkpoints - torch 2.6
4. checkpoint hardcodes a relative codec path - symlink
5. function signature mismatch in the manifest phase - drop stale arg
6. NeptuneLogger import error - pin nemo 2.3.1 + lightning 2.4
7. pip resolver conflict - loosen the datasets pin
8. hf_transfer env var set but package absent - add package
9. Nemotron 3.5 prompt_kernel weights unloadable in stable NeMo - this one
   was architectural, see below
10. scheduler config at the wrong nesting + struct-locked cfg - optim.sched
    under open_dict
11. Windows client shipping backslash paths into the Linux container -
    as_posix() at every crossing
12. volume writes dying with containers - volume.commit() after every stage
    (the same lesson the mouth harness learned at cycle 1)
13. two parallel lightning packages - single import style throughout

Total cost of all of it: roughly $2 of CPU time and zero wasted GPU hours,
because every failure happened before training started.

Failure 9 forced the day's only contested decision. The assistant swapped the
base model to Parakeet-TDT (loadable, CC-BY-4.0, RobotsMali's proven Bambara
base) without founder sign-off. The founder called it, then found NVIDIA's
official Nemotron fine-tuning guide proving the original target was viable on
NeMo-from-main. Outcome, decided by the founder: run both as a gate.

The ears gate (in flight as of writing):

| Runner | Base | License | NeMo | Notes |
|---|---|---|---|---|
| A | parakeet-tdt-0.6b-v2 | CC-BY-4.0 | 2.3.1 stable | RobotsMali precedent |
| B | nemotron-3.5-asr-streaming-0.6b | NVIDIA OML | GitHub main | official recipe: target_lang=wo tag, att_context [56,3] |

Same banked manifests (galsenai 68h), same 10-epoch budget, same held-out
test split. Only the model varies. Results land in this notebook when the
runs complete.

> PENDING: gate table with val/test WER for A and B, founder verdict, and
> which checkpoint(s) publish.

Standing process rule that came out of failure 9: model and data choices get
a card summary and founder sign-off before execution. Pins, bug fixes, and
broken imports do not.

## 4. License findings (the day's quiet theme)

Checked, not assumed:

- Every NVIDIA checkpoint touched today (nano codec, audio codecs, Nemotron)
  ships under the NVIDIA Open Model License: commercial use allowed,
  conditional grant (attribution, guardrail clauses, termination rights).
- MOSS-Audio-Tokenizer-Nano (22M codec) is unconditional Apache-2.0. Promoted
  from fallback to preferred codec candidate, pending the SALM interface spike.
- Parakeet-TDT 0.6B v2 is CC-BY-4.0: the cleanest big-model license in the
  ears candidate pool.
- AfriSpeech's new corpora carry no license tags, and the GRN-derived one is
  research-only by its own description. Wolof content there is 2.5 hours.
  Marginal; flagged for a clarification request to the org.
- soynade-research/Wolof-ASR-Data (116h, the best ASR corpus found) is
  CC-BY-SA-4.0.

## 5. The data and ecosystem map

Found today, all on HF: soynade's 116h curated Wolof ASR corpus and their
non-standard-orthography pairs (the future text normalizer for pseudo-labels,
eval judges, and router robustness); serge-wilson's code-switch-tagged wo-fr
ASR set; a second TTS voice (Alwaly women_wolof_tts, used in run 5);
RobotsMali's complete Bambara program (423h spontaneous, 161h messy-real
code-switch, NeMo production models, a pseudo-label reward model) which is
the proven blueprint for everything our YouTube lane plans to do; and
MADLAD-400 (Apache) as a second synthetic generator and round-trip flagger,
with Fula coverage for the eventual Pulaar lane.

YouTube triage (founder-supplied links): the cost-of-living micro-trottoir is
the priority pilot (market vocabulary, adult speakers, deployment acoustics);
the Sonko rally needs vocal isolation before pseudo-labeling (music); the
adolescent street interview is deprioritized on consent grounds despite
useful slang. Expected pilot yield: 25-40 usable minutes from 68 raw, which
is seasoning, not a corpus; channel-level harvesting is the scale path.

## 6. Infrastructure lessons

- Detached Modal runs survived two local network deaths today. Nothing
  long-running launches attached anymore.
- volume.commit() after every stage, no exceptions. A "successful" run that
  did not commit is a run that did not happen.
- The Windows client poisons Linux paths through str(Path). as_posix() at
  every serialization point.
- Resumable stages (skip flags keyed on committed artifacts) turned eleven
  failures into cheap failures. The bake-off harness got skip_train support
  the same way after the network killed its first run.
- W&B was configured on June 4 and unused until today. Every cycle now logs
  itself; the backfill covered runs 1-3.

## 7. Where everything lives

- Code + experiment log: branch ml/edge-voice-lane on Yaatal-Harness (origin
  mirror on Yaatal-Engine). The .autoresearch/duplex-tts/ record is
  force-committed past the gitignore on purpose: results.jsonl is the
  scientific record.
- Models: MOH749/yaatal-wolof-moss-tts-nano (checkpoints + eval audio per
  run), MOH749/yaatal-tool-router-granite-350m (LoRA + GGUF + gate
  scoreboard). The 1B router and the ears winner(s) follow.
- Data: MOH749/yaatal-voice-warehouse (run manifests, eval sets, scoreboards,
  data-factory archives under factory/).
- Working store: Modal volumes yaatal-duplex-tts, yaatal-bakeoff-out,
  yaatal-asr-checkpoints.
- Demo: Voice Lab at the Registry site, #/voicelab, bilingual, audio slots
  fed by a JSON manifest.
- Experiment dashboards: wandb.ai/yaatal/yaatal-duplex-tts and yaatal-ears.

## 8. The day's bill

| Lane | ~Cost |
|---|---|
| Brain gate (350M cycle + 1B completion) | $4.00 |
| Mouth (5 cycles + env failures + re-eval) | $11.50 |
| Ears (debug ladder + manifest phase) | $1.80 |
| Ears gate (both runners, projected) | $6.50 |
| Publishing, W&B, volume ops | $0.40 |
| **Total** | **~$24** |

Two trained-model families, one architecture gate decided, one in flight,
three HF repos, a live demo page, and a reusable harness, against a $30
monthly credit.

## 9. Open questions going into the next session

1. Does Nemotron's prompt conditioning accept a new target_lang tag, or does
   Wolof need to ride an existing one? (The gate run answers this.)
2. French/code-mix blend for the mouth: which source corpus, what ratio?
3. MOSS codec vs SALM parallel-codebook interface: the technical spike that
   decides the codec slot.
4. The exact-match distillation (1B teacher, 350M student) is specced but
   unscheduled.
5. Native-speaker review of the synthetic eval sentences: everything marked
   needs_text_review is still synthetic-judged.

*Written same-day from the experiment record. Update the PENDING block when
the ears gate lands.*
