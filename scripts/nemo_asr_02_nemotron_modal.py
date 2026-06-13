"""Ears v1: Nemotron 3.5 ASR streaming 0.6B fine-tune on Wolof (gate vs Parakeet v0).

Follows NVIDIA's official recipe (hf.co/blog/nvidia/fine-tuning-nemotron-35-asr):
  - NeMo 26.06+ (installed from GitHub main; stable pip NeMo cannot load the
    prompt_kernel weights)
  - every clip carries a target_lang tag (prompt conditioning); Wolof is new
    to the model, we introduce the consistent tag "wo"
  - streaming att_context_size "[56,3]" (~320 ms, the blog's balanced preset)
  - full fine-tune, fixed budget; eval on the same held-out split as v0

Gate fairness: trains on the SAME banked manifests as the Parakeet v0 run
(yaatal-asr-checkpoints volume), same epochs. Only the base model differs.

Launch:
  modal run --detach scripts/nemo_asr_02_nemotron_modal.py --epochs 10
"""

import json
from datetime import datetime, timezone
from pathlib import Path

import modal

APP_NAME = "yaatal-ears-nemotron"
VOLUME_NAME = "yaatal-asr-checkpoints"

volume = modal.Volume.from_name(VOLUME_NAME)

# NeMo from main: the only line that loads Nemotron 3.5's prompt_kernel weights
image = (
    modal.Image.debian_slim(python_version="3.10")
    .apt_install("git", "ffmpeg", "libsndfile1", "sox")
    .pip_install("Cython", "packaging", "huggingface-hub>=0.22", "hf_transfer",
                 "soundfile", "wandb")
    .pip_install("nemo_toolkit[asr] @ git+https://github.com/NVIDIA-NeMo/NeMo.git@main")
    # examples/ + prompt YAMLs are not in the pip package; the official recipe
    # drives NeMo's own fine-tune script with the streaming-prompt config
    .run_commands("git clone --depth 1 https://github.com/NVIDIA-NeMo/NeMo /opt/NeMo")
    .env({"HF_HUB_ENABLE_HF_TRANSFER": "1"})
)

app = modal.App(APP_NAME, image=image)

MOUNT = "/mnt/yaatal"  # plain posix strings everywhere: Windows client lesson
MANIFEST_DIR = f"{MOUNT}/output/nemo-asr"
NEMOTRON_DIR = f"{MOUNT}/output/nemotron"
BASE_MODEL = "nvidia/nemotron-3.5-asr-streaming-0.6b"
TARGET_LANG = "wo"


@app.function(volumes={MOUNT: volume}, timeout=900)
def tag_manifests() -> dict:
    """Copy the banked v0 manifests, adding the target_lang prompt tag per clip."""
    out = Path(NEMOTRON_DIR)
    out.mkdir(parents=True, exist_ok=True)
    tagged = {}
    for split in ("train", "validation", "test"):
        src = Path(MANIFEST_DIR) / f"wolof_nemo_manifest_{split}.jsonl"
        if not src.exists():
            continue
        dst = out / f"wolof_nemotron_manifest_{split}.jsonl"
        with src.open(encoding="utf-8") as f, dst.open("w", encoding="utf-8") as g:
            n = 0
            for line in f:
                if not line.strip():
                    continue
                row = json.loads(line)
                row["lang"] = TARGET_LANG          # notebook manifests carry both
                row["target_lang"] = TARGET_LANG
                g.write(json.dumps(row, ensure_ascii=False) + "\n")
                n += 1
        tagged[split] = {"path": dst.as_posix(), "rows": n}
    volume.commit()
    print(json.dumps(tagged, indent=1))
    return tagged


@app.function(gpu="A10G", volumes={MOUNT: volume},
              secrets=[modal.Secret.from_name("wandb-secret")], timeout=5 * 3600)
def finetune(epochs: int = 10, lr: float = 1e-4, batch_size: int = 8) -> dict:
    """Official recipe path: NeMo's own fine-tune script + the streaming-prompt
    YAML (which wires the prompted dataset that yields prompt_indices), seeded
    with +init_from_nemo_model per the NVIDIA notebook."""
    import subprocess
    import sys
    from huggingface_hub import snapshot_download

    manifests = {s: f"{NEMOTRON_DIR}/wolof_nemotron_manifest_{s}.jsonl"
                 for s in ("train", "validation", "test")}
    for s, p in manifests.items():
        if not Path(p).exists():
            return {"status": "error", "stage": "manifests", "error": f"missing {p}"}

    print(f"[1/4] Downloading {BASE_MODEL} .nemo checkpoint")
    ckpt_dir = Path(MOUNT) / "models" / "nemotron-3.5-asr"
    snapshot_download(BASE_MODEL, local_dir=str(ckpt_dir))
    nemo_files = list(ckpt_dir.glob("*.nemo"))
    if not nemo_files:
        return {"status": "error", "stage": "download", "error": "no .nemo in snapshot"}
    base_nemo = nemo_files[0].as_posix()
    volume.commit()

    exp_dir = Path(NEMOTRON_DIR) / "exp"
    exp_dir.mkdir(parents=True, exist_ok=True)
    cfg_dir = "/opt/NeMo/examples/asr/conf/fastconformer/cache_aware_streaming"
    script = "/opt/NeMo/examples/asr/speech_to_text_finetune.py"

    print(f"[2/4] Launching official fine-tune (target_lang={TARGET_LANG})")
    cmd = [
        sys.executable, script,
        "--config-path", cfg_dir,
        "--config-name", "fastconformer_transducer_bpe_streaming_prompt",
        f"+init_from_nemo_model={base_nemo}",
        f"++model.train_ds.manifest_filepath={manifests['train']}",
        f"++model.validation_ds.manifest_filepath={manifests['validation']}",
        f"++model.train_ds.batch_size={batch_size}",
        f"++model.validation_ds.batch_size={batch_size}",
        f"++model.optim.lr={lr}",
        "++model.optim.sched.d_model=1024",  # YAML interpolates ${model.encoder.d_model}; unresolvable here
        f"trainer.max_epochs={epochs}",
        "trainer.devices=1",
        "trainer.precision=bf16-mixed",
        f"++exp_manager.exp_dir={exp_dir.as_posix()}",
        "++exp_manager.create_wandb_logger=false",
    ]
    print(" ".join(cmd), flush=True)
    rc = subprocess.run(cmd, cwd="/opt/NeMo").returncode
    volume.commit()
    if rc != 0:
        return {"status": "error", "stage": "train", "rc": rc}

    print("[3/4] Collecting artifacts")
    nemos = sorted(exp_dir.rglob("*.nemo"), key=lambda p: p.stat().st_mtime)
    final = nemos[-1].as_posix() if nemos else None

    summary = {
        "base_model": BASE_MODEL, "target_lang": TARGET_LANG, "epochs": epochs,
        "final_ckpt": final, "exp_dir": exp_dir.as_posix(),
        "completed_at": datetime.now(timezone.utc).isoformat(),
    }
    (Path(NEMOTRON_DIR) / "run_summary.json").write_text(
        json.dumps(summary, indent=2, default=str))
    volume.commit()
    print("[4/4]", json.dumps(summary, indent=1, default=str))
    return summary


@app.local_entrypoint()
def main(epochs: int = 10, skip_tag: bool = False):
    if not skip_tag:
        tag_manifests.remote()
    print(json.dumps(finetune.remote(epochs=epochs), indent=1, default=str))
