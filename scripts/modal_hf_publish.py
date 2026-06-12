"""Publish Yaatal edge-voice artifacts from Modal volumes to HuggingFace.

Creates/updates three private repos under the token's namespace:
  - yaatal-wolof-moss-tts-nano  (model: SFT checkpoints + eval audio + scoreboards)
  - yaatal-tool-router-granite-350m  (model: qualified LoRA + Q4 GGUF + scoreboard)
  - yaatal-voice-warehouse  (dataset: manifests, eval sentences, run records)

Usage:
  modal run scripts/modal_hf_publish.py --run-label run1-baseline --include-router
"""

import os
import json
from pathlib import Path

import modal

app = modal.App("yaatal-hf-publish")
tts_vol = modal.Volume.from_name("yaatal-duplex-tts")
bakeoff_vol = modal.Volume.from_name("yaatal-bakeoff-out")

image = modal.Image.debian_slim(python_version="3.11").pip_install("huggingface_hub")

TTS_CARD = """---
license: apache-2.0
language: [wo]
base_model: OpenMOSS-Team/MOSS-TTS-Nano-100M
pipeline_tag: text-to-speech
---
# Yaatal Wolof MOSS-TTS-Nano

Wolof SFT checkpoints of MOSS-TTS-Nano-100M (Apache-2.0), trained on
[galsenai/wolof_tts](https://hf.co/datasets/galsenai/wolof_tts) (CC-BY-4.0,
Baamtu Datamation / AI4D — attribution required). Part of the YAATAL
sovereign edge-voice stack (ears: NeMo ASR; brain: Granite 350M tool-router;
mouth: this model). Checkpoints under `checkpoints/<run>/`, eval audio and
scoreboards under `eval/<run>/`. Research checkpoints — see scoreboards for
per-run quality before use.
"""

ROUTER_CARD = """---
license: apache-2.0
language: [wo, fr]
base_model: ibm-granite/granite-4.0-h-350m
---
# Yaatal Tool-Router — Granite 4.0-H 350M (qualified 2026-06-12)

LoRA + Q4_K_M GGUF (~210 MB) for market-intent extraction (Wolof/French
code-mix -> JSON). Gate result vs granite-4.0-h-1b on the same v2 held-out:
slot_f1 0.846 vs 0.879 (<=5% gate passed), intent accuracy 0.993 vs 0.960,
JSON validity 1.0. Default edge backbone of the YAATAL stack.
"""


@app.function(image=image, volumes={"/tts": tts_vol, "/bakeoff": bakeoff_vol},
              secrets=[modal.Secret.from_name("huggingface-secret")], timeout=3600)
def publish(run_label: str, include_router: bool = False) -> dict:
    from huggingface_hub import HfApi

    token = (os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_TOKEN")
             or os.environ.get("HUGGING_FACE_HUB_TOKEN"))
    api = HfApi(token=token)
    user = api.whoami()["name"]
    out = {"namespace": user, "published": []}

    # --- TTS model repo -------------------------------------------------------
    tts_repo = f"{user}/yaatal-wolof-moss-tts-nano"
    api.create_repo(tts_repo, private=True, exist_ok=True)
    api.upload_file(path_or_fileobj=TTS_CARD.encode(), path_in_repo="README.md",
                    repo_id=tts_repo)
    run_dir = Path("/tts/runs") / run_label
    ckpt = run_dir / "sft" / "checkpoint-last"
    if ckpt.exists():
        api.upload_folder(folder_path=str(ckpt), repo_id=tts_repo,
                          path_in_repo=f"checkpoints/{run_label}")
        out["published"].append(f"{tts_repo}/checkpoints/{run_label}")
    for sub in ("eval_audio", "scoreboard.json", "eval_sentences.json"):
        p = run_dir / sub
        if p.is_dir():
            api.upload_folder(folder_path=str(p), repo_id=tts_repo,
                              path_in_repo=f"eval/{run_label}/{sub}")
        elif p.exists():
            api.upload_file(path_or_fileobj=str(p), repo_id=tts_repo,
                            path_in_repo=f"eval/{run_label}/{sub}")
    out["published"].append(f"{tts_repo}/eval/{run_label}")

    # --- Router model repo ----------------------------------------------------
    if include_router:
        r_repo = f"{user}/yaatal-tool-router-granite-350m"
        api.create_repo(r_repo, private=True, exist_ok=True)
        api.upload_file(path_or_fileobj=ROUTER_CARD.encode(),
                        path_in_repo="README.md", repo_id=r_repo)
        base = Path("/bakeoff/bakeoff/granite-4.0-h-350m-v2")
        for sub in ("lora", "gguf", "reports"):
            if (base / sub).exists():
                api.upload_folder(folder_path=str(base / sub), repo_id=r_repo,
                                  path_in_repo=sub)
        out["published"].append(r_repo)

    # --- Dataset warehouse ----------------------------------------------------
    d_repo = f"{user}/yaatal-voice-warehouse"
    api.create_repo(d_repo, private=True, exist_ok=True, repo_type="dataset")
    for name in ("train_raw.jsonl", "eval_sentences.json", "scoreboard.json"):
        p = run_dir / name
        if p.exists():
            api.upload_file(path_or_fileobj=str(p), repo_id=d_repo,
                            repo_type="dataset",
                            path_in_repo=f"duplex-tts/{run_label}/{name}")
    out["published"].append(f"{d_repo}/duplex-tts/{run_label}")

    print(json.dumps(out, indent=1))
    return out


@app.local_entrypoint()
def main(run_label: str = "run1-baseline", include_router: bool = False):
    print(json.dumps(publish.remote(run_label, include_router), indent=1))
