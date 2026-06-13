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
    import torch
    import lightning.pytorch as pl  # single import style, NeMo's base
    from lightning.pytorch.callbacks import LearningRateMonitor, ModelCheckpoint
    from omegaconf import OmegaConf, open_dict
    from nemo.collections.asr.models import ASRModel

    manifests = {s: f"{NEMOTRON_DIR}/wolof_nemotron_manifest_{s}.jsonl"
                 for s in ("train", "validation", "test")}
    for s, p in manifests.items():
        if not Path(p).exists():
            return {"status": "error", "stage": "manifests", "error": f"missing {p}"}

    print(f"[1/5] Loading {BASE_MODEL} (NeMo main)")
    model = ASRModel.from_pretrained(model_name=BASE_MODEL, map_location="cpu")
    print(f"[OK] {type(model).__name__} loaded")

    cfg = model.cfg
    with open_dict(cfg):
        ds_common = {
            "sample_rate": 16000, "batch_size": batch_size, "num_workers": 4,
            "max_duration": 25.0, "min_duration": 0.1, "shuffle": True,
            "is_tarred": False, "use_start_end_token": True,
        }
        cfg.train_ds = OmegaConf.create({**ds_common,
                                         "manifest_filepath": manifests["train"]})
        cfg.validation_ds = OmegaConf.create({**ds_common, "shuffle": False,
                                              "manifest_filepath": manifests["validation"]})
        cfg.optim = OmegaConf.create({
            "name": "adamw", "lr": lr, "betas": [0.9, 0.98], "weight_decay": 0.001,
            "sched": {"name": "CosineAnnealing", "max_steps": epochs * 1000,
                      "min_lr": lr * 0.01, "warmup_steps": 500},
        })
        # blog's balanced streaming preset (~320 ms)
        if hasattr(model, "encoder") and hasattr(model.encoder, "att_context_size"):
            cfg.encoder.att_context_size = [56, 3]

    model.setup_training_data(train_data_config=cfg.train_ds)
    model.setup_validation_data(val_data_config=cfg.validation_ds)
    model.setup_optimization(optim_config=cfg.optim)

    out_dir = Path(NEMOTRON_DIR) / "checkpoints"
    out_dir.mkdir(parents=True, exist_ok=True)
    trainer = pl.Trainer(
        max_epochs=epochs, accelerator="gpu", devices=1, precision="bf16-mixed",
        log_every_n_steps=50, val_check_interval=1.0, num_sanity_val_steps=2,
        callbacks=[LearningRateMonitor(),
                   ModelCheckpoint(dirpath=str(out_dir), save_top_k=1,
                                   monitor="val_wer", mode="min")],
    )
    model.set_trainer(trainer)

    print(f"[2/5] Training {epochs} epochs, target_lang={TARGET_LANG}")
    trainer.fit(model)

    print("[3/5] Validation")
    val = trainer.validate(model)
    val_wer = val[0].get("val_wer") if val else None

    print("[4/5] Test")
    with open_dict(cfg):
        cfg.test_ds = OmegaConf.create({**{k: v for k, v in
                                           dict(cfg.validation_ds).items()},
                                        "manifest_filepath": manifests["test"]})
    model.setup_test_data(test_data_config=cfg.test_ds)
    test = trainer.test(model)

    print("[5/5] Save")
    final = out_dir / "nemotron_wolof_v1.nemo"
    model.save_to(str(final))
    summary = {
        "base_model": BASE_MODEL, "target_lang": TARGET_LANG, "epochs": epochs,
        "val_wer": val_wer, "test": test, "final_ckpt": final.as_posix(),
        "completed_at": datetime.now(timezone.utc).isoformat(),
    }
    (out_dir / "run_summary.json").write_text(json.dumps(summary, indent=2, default=str))
    volume.commit()
    try:
        import wandb
        w = wandb.init(project="yaatal-ears", name="nemotron-v1-wolof",
                       config={"base": BASE_MODEL, "epochs": epochs, "lr": lr})
        w.log({"val_wer": val_wer or -1})
        w.finish()
    except Exception as e:
        print(f"[wandb] skipped: {e}")
    print(json.dumps(summary, indent=1, default=str))
    return summary


@app.local_entrypoint()
def main(epochs: int = 10, skip_tag: bool = False):
    if not skip_tag:
        tag_manifests.remote()
    print(json.dumps(finetune.remote(epochs=epochs), indent=1, default=str))
