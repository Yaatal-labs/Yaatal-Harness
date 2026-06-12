"""Yaatal duplex-edge autoresearch cycle: MOSS-TTS-Nano Wolof SFT + eval on Modal.

One call = one autoresearch cycle:
  data (galsenai/wolof_tts -> raw JSONL) -> prepare_data.py (codec tokens)
  -> sft.py (full SFT, ~3.2 GiB VRAM) -> eval (gen 20 sents, ASR round-trip CER,
  SQUIM STOI, sanity, duplex loop with the qualified Granite-350M router LoRA)
  -> scoreboard JSON committed to the volume.

Launch (detached; survives flaky local network):
  modal run --detach scripts/modal_moss_tts_wolof.py --recipe .autoresearch/duplex-tts/prompt.txt --run-label run1
Stages are resumable: completed stage outputs in the volume are reused.
"""

import json
import subprocess
import sys
from pathlib import Path

import modal

app = modal.App("yaatal-moss-tts-wolof")
vol = modal.Volume.from_name("yaatal-duplex-tts", create_if_missing=True)
bakeoff_vol = modal.Volume.from_name("yaatal-bakeoff-out")

image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install("git", "ffmpeg")
    .pip_install(
        # mirror MOSS-TTS-Nano requirements.txt exactly (torch/transformers pins)
        "torch==2.7.0", "torchaudio==2.7.0",
        "transformers==4.57.1", "accelerate>=1.0.0", "datasets==2.21.0",
        "soundfile", "librosa", "jiwer", "peft", "tqdm", "safetensors",
        "huggingface_hub", "sentencepiece", "protobuf", "wandb", "WeTextProcessing>=1.0.4.1",
    )
    .run_commands("git clone --depth 1 https://github.com/OpenMOSS/MOSS-TTS-Nano /opt/moss")
)

V = "/vol"  # volume mount


def _run(tag: str, cmd: list, cwd: str = "/opt/moss") -> int:
    print(f"=== [{tag}] {' '.join(str(c) for c in cmd)}", flush=True)
    p = subprocess.run(cmd, cwd=cwd)
    print(f"=== [{tag}] exit={p.returncode}", flush=True)
    return p.returncode


@app.function(image=image, gpu="A10G", volumes={V: vol, "/bakeoff": bakeoff_vol},
              secrets=[modal.Secret.from_name("wandb-secret")],
              timeout=4 * 3600)
def run_cycle(recipe: dict, run_label: str, market_turns: list) -> dict:
    import torch
    from huggingface_hub import snapshot_download

    out = Path(V) / "runs" / run_label
    out.mkdir(parents=True, exist_ok=True)
    sb = {"run_label": run_label, "recipe": recipe, "stages": {}}

    # -- stage 0: models (cached in volume) -----------------------------------
    models = Path(V) / "models"
    tts_dir, codec_dir = models / "moss-tts-nano", models / "moss-codec-nano"
    for repo, d in [(recipe["base_model"], tts_dir), (recipe["codec_model"], codec_dir)]:
        if not (d / "config.json").exists():
            snapshot_download(repo, local_dir=str(d))
    vol.commit()
    sb["stages"]["models"] = "ok"

    # -- stage 1: dataset -> raw JSONL + eval sentences -----------------------
    data_dir = Path(V) / "data" / recipe["dataset"].replace("/", "__")
    raw_jsonl = out / "train_raw.jsonl"
    val_path = out / "eval_sentences.json"
    if not raw_jsonl.exists():
        from datasets import load_dataset, Audio
        import soundfile as sf

        # dataset mix: primary + extra_datasets, budget split evenly across sources.
        # extras may carry a quality tag ("name:quality") passed into the MOSS JSONL.
        sources = [(recipe["dataset"], None)]
        for spec in recipe.get("extra_datasets", []):
            name, _, q = spec.partition(":")
            sources.append((name, q or None))
        budget = recipe["max_train_clips"] // len(sources)
        rows, held_out = [], []
        for si, (ds_name, quality) in enumerate(sources):
            ds = load_dataset(ds_name, split="train")
            ds = ds.cast_column("audio", Audio(sampling_rate=24000))
            ds = ds.shuffle(seed=42)
            n = min(budget + (20 if si == 0 else 0), len(ds))
            wav_dir = Path(V) / "data" / ds_name.replace("/", "__") / "wav"
            wav_dir.mkdir(parents=True, exist_ok=True)
            text_key = next(k for k in ("text", "transcription", "sentence", "transcript")
                            if k in ds.column_names)
            for i, ex in enumerate(ds.select(range(n))):
                txt = (ex[text_key] or "").strip()
                if not txt:
                    continue
                if si == 0 and len(held_out) < 10:  # held-out only from primary
                    held_out.append(txt)
                    continue
                wav = wav_dir / f"clip_{i:06d}.wav"
                if not wav.exists():
                    sf.write(str(wav), ex["audio"]["array"], ex["audio"]["sampling_rate"])
                row = {"audio": str(wav), "text": txt, "language": recipe["language_tag"]}
                if quality:
                    row["quality"] = quality
                rows.append(row)
        raw_jsonl.write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows),
                             encoding="utf-8")
        # eval set: 10 fixed validation (5 held-out + 5 market) + 10 rotating
        market = [t["text"] for t in market_turns][:10]
        eval_sents = {
            "validation": held_out[:5] + market[:5],
            "rotating": held_out[5:10] + market[5:10],
        }
        val_path.write_text(json.dumps(eval_sents, ensure_ascii=False, indent=1),
                            encoding="utf-8")
        vol.commit()
    sb["stages"]["data"] = "ok"

    # -- stage 2: codec tokenization ------------------------------------------
    prep_jsonl = out / "train_with_codes.jsonl"
    if not prep_jsonl.exists():
        rc = _run("prepare", [sys.executable, "finetuning/prepare_data.py",
                              "--codec-path", str(codec_dir),
                              "--input-jsonl", str(raw_jsonl),
                              "--output-jsonl", str(prep_jsonl),
                              "--batch-size", "8",
                              "--skip-reference-audio-codes"])
        vol.commit()
        if rc != 0 or not prep_jsonl.exists():
            sb["stages"]["prepare"] = f"FAIL rc={rc}"
            (out / "scoreboard.json").write_text(json.dumps(sb, indent=1))
            vol.commit()
            return sb
    sb["stages"]["prepare"] = "ok"

    # -- stage 3: SFT ----------------------------------------------------------
    ckpt_root = out / "sft"
    done_flag = out / "train.done"
    if not done_flag.exists():
        rc = _run("sft", ["accelerate", "launch", "finetuning/sft.py",
                          "--model-path", str(tts_dir),
                          "--codec-path", str(codec_dir),
                          "--train-jsonl", str(prep_jsonl),
                          "--output-dir", str(ckpt_root),
                          "--per-device-batch-size", str(recipe["per_device_batch_size"]),
                          "--gradient-accumulation-steps", str(recipe["grad_accum"]),
                          "--learning-rate", str(recipe["learning_rate"]),
                          "--warmup-ratio", "0.03",
                          "--num-epochs", str(recipe["num_epochs"]),
                          "--mixed-precision", "bf16",
                          "--max-length", str(recipe["max_length"]),
                          "--channelwise-loss-weight", recipe["channelwise_loss_weight"]])
        vol.commit()
        if rc != 0:
            sb["stages"]["sft"] = f"FAIL rc={rc}"
            (out / "scoreboard.json").write_text(json.dumps(sb, indent=1))
            vol.commit()
            return sb
        done_flag.write_text("ok")
        vol.commit()
    ckpts = sorted(ckpt_root.glob("checkpoint*"))
    ckpt = str(ckpts[-1]) if ckpts else str(ckpt_root)
    sb["stages"]["sft"] = f"ok ckpt={ckpt}"

    # -- stage 4: eval ----------------------------------------------------------
    import torchaudio
    import numpy as np

    # checkpoint modeling code resolves the codec at ./models/MOSS-Audio-Tokenizer-Nano
    # relative to the repo cwd; satisfy it with a symlink to our volume copy
    moss_models = Path("/opt/moss/models")
    moss_models.mkdir(exist_ok=True)
    link = moss_models / "MOSS-Audio-Tokenizer-Nano"
    if not link.exists():
        link.symlink_to(codec_dir)

    eval_sents = json.loads(val_path.read_text(encoding="utf-8"))
    all_sents = eval_sents["validation"] + eval_sents["rotating"]
    audio_dir = out / "eval_audio"
    audio_dir.mkdir(exist_ok=True)

    # voice_clone mode anchors generation to a native reference clip (recipe
    # "eval_mode": "voice_clone"); bare continuation invents a voice from nothing
    eval_mode = recipe.get("eval_mode", "continuation")
    prompt_args = []
    if eval_mode == "voice_clone":
        first_row = json.loads(raw_jsonl.read_text(encoding="utf-8").splitlines()[0])
        prompt_args = ["--prompt-audio-path", first_row["audio"]]

    gen_ok, durs, rms_vals, paths = 0, [], [], []
    for i, txt in enumerate(all_sents):
        wav_path = audio_dir / f"eval_{i:02d}.wav"
        rc = _run(f"gen{i}", [sys.executable, "finetuning/verify.py",
                              "--checkpoint", ckpt, "--mode", eval_mode,
                              "--text", txt, *prompt_args,
                              "--output-audio-path", str(wav_path)])
        if rc == 0 and wav_path.exists():
            gen_ok += 1
            paths.append((i, txt, wav_path))
            w, sr = torchaudio.load(str(wav_path))
            durs.append(w.shape[-1] / sr)
            rms_vals.append(float(w.pow(2).mean().sqrt()))
        else:
            paths.append((i, txt, None))
            durs.append(0.0)
            rms_vals.append(0.0)
    vol.commit()

    sanity = sum(1 for d, r in zip(durs, rms_vals) if 0.4 <= d <= 25.0 and r > 0.005)

    # ASR round-trip CER
    cers, transcripts = [], []
    try:
        from transformers import pipeline
        asr = pipeline("automatic-speech-recognition", model=recipe["asr_model"],
                       device=0 if torch.cuda.is_available() else -1)
        import jiwer
        norm = lambda s: " ".join("".join(c.lower() for c in s if c.isalnum() or c.isspace()).split())
        for i, txt, wp in paths:
            if wp is None:
                cers.append(1.0); transcripts.append("")
                continue
            hyp = asr(str(wp))["text"]
            transcripts.append(hyp)
            ref = norm(txt)
            cers.append(jiwer.cer(ref, norm(hyp)) if ref else 1.0)
    except Exception as e:  # ASR judge failing != TTS failing; record and move on
        sb["stages"]["asr"] = f"FAIL {e}"
        cers = [1.0] * len(paths)
        transcripts = [""] * len(paths)

    # SQUIM objective STOI (reference-free quality proxy)
    stois = []
    try:
        sq = torchaudio.pipelines.SQUIM_OBJECTIVE.get_model()
        for i, txt, wp in paths:
            if wp is None:
                continue
            w, sr = torchaudio.load(str(wp))
            w = torchaudio.functional.resample(w.mean(0, keepdim=True), sr, 16000)
            stoi, _, _ = sq(w)
            stois.append(float(stoi))
    except Exception as e:
        sb["stages"]["squim"] = f"FAIL {e}"

    # duplex loop: router LoRA (qualified brain) -> intent JSON -> spoken Wolof ack
    duplex_pass, duplex_log = 0, []
    try:
        from transformers import AutoModelForCausalLM, AutoTokenizer
        from peft import PeftModel
        base = "ibm-granite/granite-4.0-h-350m"
        tok = AutoTokenizer.from_pretrained(base)
        m = AutoModelForCausalLM.from_pretrained(base, torch_dtype=torch.bfloat16,
                                                 device_map="cuda")
        m = PeftModel.from_pretrained(m, "/bakeoff/bakeoff/granite-4.0-h-350m-v2/lora")
        user_turns = [t["text"] for t in market_turns][:5]
        acks = {None: "Waaw, ma seet li nga laaj."}
        for j, ut in enumerate(user_turns):
            msgs = [{"role": "system",
                     "content": "You are a tool router. Output ONLY a JSON object for the user's market request."},
                    {"role": "user", "content": ut}]
            ids = tok.apply_chat_template(msgs, return_tensors="pt",
                                          add_generation_prompt=True).to("cuda")
            gen = m.generate(ids, max_new_tokens=200, do_sample=False)
            resp = tok.decode(gen[0][ids.shape[-1]:], skip_special_tokens=True)
            ok_json = False
            try:
                s, e = resp.index("{"), resp.rindex("}") + 1
                json.loads(resp[s:e]); ok_json = True
            except Exception:
                pass
            wav_path = audio_dir / f"duplex_{j}.wav"
            rc = _run(f"duplex{j}", [sys.executable, "finetuning/verify.py",
                                     "--checkpoint", ckpt, "--mode", eval_mode,
                                     "--text", acks[None], *prompt_args,
                                     "--output-audio-path", str(wav_path)])
            spoke = rc == 0 and wav_path.exists()
            duplex_pass += int(ok_json and spoke)
            duplex_log.append({"turn": ut[:60], "json_ok": ok_json, "spoke": spoke,
                               "raw": resp[:200]})
    except Exception as e:
        sb["stages"]["duplex"] = f"FAIL {e}"

    n_val = len(eval_sents["validation"])
    th = recipe["thresholds"]
    med = lambda xs: float(np.median(xs)) if xs else 1.0
    crit = {
        "gen_success": gen_ok == len(all_sents),
        "asr_roundtrip": med(cers) <= th["cer_median"],
        "audio_sanity": sanity >= th["sanity_min_of_20"],
        "stoi_quality": (float(np.mean(stois)) if stois else 0.0) >= th["stoi_mean"],
        "duplex_e2e": duplex_pass >= th["duplex_min_of_5"],
    }
    crit_val = {  # same criteria computed on the 10 fixed validation sentences
        "gen_success": all(p[2] is not None for p in paths[:n_val]),
        "asr_roundtrip": med(cers[:n_val]) <= th["cer_median"],
        "audio_sanity": sum(1 for d, r in zip(durs[:n_val], rms_vals[:n_val])
                            if 0.4 <= d <= 25.0 and r > 0.005) >= n_val - 1,
    }
    sb.update({
        "criteria": crit, "criteria_validation": crit_val,
        "metrics": {"gen_ok": gen_ok, "cer_median": med(cers),
                    "cer_validation_median": med(cers[:n_val]),
                    "stoi_mean": float(np.mean(stois)) if stois else None,
                    "sanity": sanity, "duplex_pass": duplex_pass,
                    "durations": durs, "rms": rms_vals},
        "transcripts": [{"text": t, "asr": tr, "cer": c}
                        for (_, t, _), tr, c in zip(paths, transcripts, cers)],
        "duplex_log": duplex_log,
    })
    (out / "scoreboard.json").write_text(json.dumps(sb, ensure_ascii=False, indent=1),
                                         encoding="utf-8")
    vol.commit()
    try:  # experiment tracking; never let it fail the cycle
        import wandb
        w = wandb.init(project="yaatal-duplex-tts", name=run_label, config=recipe)
        w.log({**{f"criteria/{k}": int(v) for k, v in crit.items()},
               "cer_median": sb["metrics"]["cer_median"],
               "cer_validation_median": sb["metrics"]["cer_validation_median"],
               "stoi_mean": sb["metrics"]["stoi_mean"] or 0.0,
               "sanity": sanity, "gen_ok": gen_ok, "duplex_pass": duplex_pass})
        w.finish()
    except Exception as e:
        print(f"[wandb] skipped: {e}")
    print("SCOREBOARD:", json.dumps({k: v for k, v in sb.items()
                                     if k in ("criteria", "metrics")}, indent=1))
    return sb


@app.function(image=image, volumes={V: vol},
              secrets=[modal.Secret.from_name("wandb-secret")], timeout=600)
def backfill_wandb() -> list:
    """One wandb run per existing scoreboard in the volume (runs that predate tracking)."""
    import wandb

    logged = []
    for sb_path in sorted(Path(V).glob("runs/*/scoreboard.json")):
        sb = json.loads(sb_path.read_text(encoding="utf-8"))
        if "metrics" not in sb:
            continue
        m = sb["metrics"]
        w = wandb.init(project="yaatal-duplex-tts",
                       name=f"backfill-{sb['run_label']}", config=sb.get("recipe", {}))
        w.log({**{f"criteria/{k}": int(v) for k, v in sb.get("criteria", {}).items()},
               "cer_median": m["cer_median"],
               "cer_validation_median": m.get("cer_validation_median", 1.0),
               "stoi_mean": m.get("stoi_mean") or 0.0,
               "sanity": m["sanity"], "gen_ok": m["gen_ok"],
               "duplex_pass": m["duplex_pass"]})
        w.finish()
        logged.append(sb["run_label"])
    return logged


@app.local_entrypoint()
def main(recipe: str = ".autoresearch/duplex-tts/prompt.txt",
         run_label: str = "run1",
         market_manifest: str = "output/yaatal-data-factory/tts/tts_input_manifest.jsonl",
         backfill: bool = False):
    if backfill:
        print(json.dumps(backfill_wandb.remote(), indent=1))
        return
    rec = json.loads(Path(recipe).read_text(encoding="utf-8"))
    turns = [json.loads(l) for l in
             Path(market_manifest).read_text(encoding="utf-8").splitlines() if l.strip()]
    sb = run_cycle.remote(rec, run_label, turns)
    print(json.dumps({k: v for k, v in sb.items() if k != "transcripts"},
                     ensure_ascii=False, indent=1))
