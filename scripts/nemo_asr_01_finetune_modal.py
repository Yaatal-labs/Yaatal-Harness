# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "modal>=0.63",
# ]
# ///
"""YAATAL — Modal GPU runner for Nemotron 3.5 ASR Wolof fine-tuning.

This script wraps the entire pipeline:
  1. Download galsenai/wolof-audio-data from HuggingFace
  2. Convert to NeMo ASR manifests
  3. Fine-tune nvidia/nemotron-3.5-asr-streaming-0.6b
  4. Save checkpoints to a Modal volume
  5. Run WER evaluation on test split

Usage:
    modal run scripts/nemo_asr_01_finetune_modal.py \
        --epochs 10 \
        --batch-size 8 \
        --exp-name wolof_nemotron35_v1

Requirements:
    - MODAL_TOKEN_ID and MODAL_TOKEN_SECRET in environment
    - HF_TOKEN in environment (for gated model download if needed)
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import modal

# ── Modal App Configuration ───────────────────────────────────────────────────

APP_NAME = "yaatal-asr-finetune"
VOLUME_NAME = "yaatal-asr-checkpoints"

# Persistent volume for checkpoints and datasets
volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)

# Base image with NeMo toolkit + dependencies
image = (
    modal.Image.debian_slim(python_version="3.10")
    .apt_install("git", "ffmpeg", "libsndfile1")
    .pip_install(
        "nemo-toolkit[asr]==2.3.1",
        "huggingface-hub>=0.22", "hf_transfer",
        "datasets>=4.4",
        "soundfile>=0.12",
        "librosa>=0.10",
        "torch==2.6.0",
        "torchaudio==2.6.0",
        "pytorch-lightning==2.4.0",
        "omegaconf>=2.3",
        "wandb>=0.16",
        "numpy>=1.26",
        "scipy>=1.11",
    )
    .env({"HF_HUB_ENABLE_HF_TRANSFER": "1"})
)

app = modal.App(APP_NAME, image=image)

# ── Constants ─────────────────────────────────────────────────────────────────

DATASET_NAME = "galsenai/wolof-audio-data"
# Nemotron 3.5 streaming carries prompt_kernel weights that stable NeMo (2.3.x)
# cannot load; Parakeet-TDT is the RobotsMali-proven base for this NeMo line
# (their Bambara soloni models). Revisit Nemotron when NeMo catches up.
BASE_MODEL = "nvidia/parakeet-tdt-0.6b-v2"
MOUNT_PATH = Path("/mnt/yaatal")
OUTPUT_DIR = MOUNT_PATH / "output"
CHECKPOINT_DIR = OUTPUT_DIR / "nemo-asr" / "checkpoints"
MANIFEST_DIR = OUTPUT_DIR / "nemo-asr"


# ── Helper Functions ─────────────────────────────────────────────────────────

def _run_cmd(cmd: list[str], cwd: Path | None = None, env: dict | None = None) -> None:
    """Run a shell command and stream output."""
    merged_env = {**os.environ, **(env or {})}
    print(f"[CMD] {' '.join(cmd)}")
    result = subprocess.run(cmd, cwd=cwd, env=merged_env, check=True, capture_output=False, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}")


# ── Modal Function: Create Manifests ────────────────────────────────────────

@app.function(
    gpu=None,  # CPU-only
    volumes={str(MOUNT_PATH): volume},
    timeout=1800,  # 30 min
)
def create_manifests(
    dataset_name: str = DATASET_NAME,
    output_dir: str = str(MANIFEST_DIR),
    val_fraction: float = 0.1,
    test_fraction: float = 0.1,
) -> dict:
    """Download dataset and create NeMo ASR manifests."""
    import json
    import hashlib
    import io
    from collections import defaultdict

    import soundfile as sf
    from datasets import load_dataset, Audio

    print(f"[1/3] Loading dataset: {dataset_name}")
    
    # Load with non-streaming (downloads full parquet first — more reliable)
    ds = load_dataset(dataset_name, split=None)
    
    out_path = Path(output_dir)
    audio_dir = out_path / "audio"
    out_path.mkdir(parents=True, exist_ok=True)
    audio_dir.mkdir(parents=True, exist_ok=True)

    def _process_split(split_ds, split_tag: str):
        rows = []
        kept = 0
        skipped = 0
        total_dur = 0.0
        
        for ex in split_ds:
            text = ex.get("sentence", "") or ex.get("transcription", "") or ""
            if not text:
                skipped += 1
                continue
                
            audio = ex.get("audio", {})
            data = audio.get("bytes")
            if not data and audio.get("path"):
                p = Path(audio["path"])
                if p.exists():
                    data = p.read_bytes()
            if not data:
                skipped += 1
                continue
            
            try:
                arr, sr = sf.read(io.BytesIO(data), dtype="float32")
                duration = float(len(arr)) / float(sr) if sr else 0.0
                if duration <= 0.0:
                    skipped += 1
                    continue
            except Exception:
                skipped += 1
                continue
            
            wav_name = f"{split_tag}_{kept:08d}.wav"
            wav_path = audio_dir / wav_name
            sf.write(str(wav_path), arr, sr)
            
            rows.append({
                "audio_filepath": str(wav_path.resolve()),
                "duration": round(duration, 3),
                "text": text,
                "lang": "wo",
            })
            kept += 1
            total_dur += duration
        
        return rows, {
            "count": kept,
            "skipped": skipped,
            "hours": total_dur / 3600.0,
            "avg_sec": total_dur / kept if kept else 0.0,
        }

    # Process available splits
    raw_by_split: dict[str, list] = {}
    for split_name in ds.keys():
        print(f"[2/3] Processing split '{split_name}'...")
        split_ds = ds[split_name].cast_column("audio", Audio(decode=False))
        rows, stats = _process_split(split_ds, split_name)
        raw_by_split[split_name] = rows
        print(f"  Kept: {stats['count']}, Skipped: {stats['skipped']}, Hours: {stats['hours']:.2f}")

    # Normalize splits
    final: dict[str, list] = {}
    if "train" in raw_by_split and "test" in raw_by_split:
        # Carve validation from train
        train_rows = raw_by_split["train"]
        val_rows, new_train = [], []
        for r in train_rows:
            bucket = int(hashlib.sha1(r["audio_filepath"].encode()).hexdigest(), 16) % 100
            if bucket < val_fraction * 100:
                val_rows.append(r)
            else:
                new_train.append(r)
        final["train"] = new_train
        final["validation"] = val_rows
        final["test"] = raw_by_split["test"]
    else:
        # Single split — hash-split all
        all_rows = list(raw_by_split.values())[0]
        train_rows, val_rows, test_rows = [], [], []
        for r in all_rows:
            bucket = int(hashlib.sha1(r["audio_filepath"].encode()).hexdigest(), 16) % 100
            if bucket < test_fraction * 100:
                test_rows.append(r)
            elif bucket < (test_fraction + val_fraction) * 100:
                val_rows.append(r)
            else:
                train_rows.append(r)
        final["train"] = train_rows
        final["validation"] = val_rows
        final["test"] = test_rows

    # Write manifests
    manifest_paths = {}
    for split_name, rows in final.items():
        if not rows:
            continue
        p = out_path / f"wolof_nemo_manifest_{split_name}.jsonl"
        with p.open("w", encoding="utf-8") as f:
            for r in rows:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
        manifest_paths[split_name] = str(p)
        print(f"[3/3] Wrote {p} ({len(rows)} entries)")

    # Summary
    total_hours = sum(sum(r["duration"] for r in rows) / 3600.0 for rows in final.values())
    total_samples = sum(len(rows) for rows in final.values())
    
    summary = {
        "dataset": dataset_name,
        "total_hours": round(total_hours, 2),
        "total_samples": total_samples,
        "manifests": manifest_paths,
    }
    
    summary_path = out_path / "manifest_summary.json"
    with summary_path.open("w") as f:
        json.dump(summary, f, indent=2)
    
    print(f"\n[OK] Total: {total_samples} samples | {total_hours:.2f} hours")
    return summary


# ── Modal Function: Fine-tune ASR ────────────────────────────────────────────

@app.function(
    gpu="A10G",  # or "A100-40G" for larger batches
    volumes={str(MOUNT_PATH): volume},
    timeout=14400,  # 4 hours
)
def finetune_asr(
    train_manifest: str,
    val_manifest: str,
    test_manifest: str | None = None,
    epochs: int = 10,
    batch_size: int = 8,
    lr: float = 1e-5,
    exp_name: str = "wolof_nemotron35_asr",
    use_lora: bool = False,
) -> dict:
    """Fine-tune Nemotron 3.5 ASR on NeMo manifests."""
    import json
    import time
    import traceback
    from datetime import datetime, timezone

    import pytorch_lightning as pl
    from nemo.collections.asr.models import ASRModel
    from nemo.utils import logging as nemo_logging
    from omegaconf import OmegaConf
    from pytorch_lightning.callbacks import EarlyStopping, LearningRateMonitor, ModelCheckpoint

    # Paths
    ckpt_dir = CHECKPOINT_DIR / exp_name
    version_dir = ckpt_dir / f"run_{datetime.now(timezone.utc).strftime('%Y%m%d_%H%M%S')}"
    version_dir.mkdir(parents=True, exist_ok=True)

    # Logging
    def json_log(step: int, payload: dict):
        (version_dir / "metrics.jsonl").open("a").write(
            json.dumps({"step": step, **payload}, ensure_ascii=False) + "\n"
        )

    pl.seed_everything(42, workers=True)
    nemo_logging.setLevel("INFO")

    # Load base model
    print(f"[1/5] Loading base model: {BASE_MODEL}")
    start = time.time()
    try:
        # ASRModel resolves the checkpoint's own class (TDT/hybrid/RNNT variants)
        model = ASRModel.from_pretrained(model_name=BASE_MODEL, map_location="cpu")
    except Exception as exc:
        print(f"[FATAL] Model load failed: {exc}")
        traceback.print_exc()
        return {"status": "error", "stage": "model_load", "error": str(exc)}

    print(f"[OK] Model loaded in {time.time() - start:.1f}s")
    json_log(0, {"event": "model_loaded", "duration_sec": round(time.time() - start, 2)})

    # Config overrides
    cfg = model.cfg
    cfg.train_ds = OmegaConf.create({
        "manifest_filepath": train_manifest,
        "sample_rate": 16000,
        "batch_size": batch_size,
        "shuffle": True,
        "num_workers": 4,
        "pin_memory": True,
        "max_duration": 25.0,
        "min_duration": 0.1,
        "trim_silence": False,
        "use_start_end_token": True,
        "is_tarred": False,
    })
    cfg.validation_ds = OmegaConf.create({
        "manifest_filepath": val_manifest,
        "sample_rate": 16000,
        "batch_size": batch_size,
        "shuffle": False,
        "num_workers": 4,
        "pin_memory": True,
        "max_duration": 25.0,
        "min_duration": 0.1,
        "trim_silence": False,
        "use_start_end_token": True,
        "is_tarred": False,
    })
    cfg.spec_augment = OmegaConf.create({
        "_target_": "nemo.collections.asr.modules.SpectrogramAugmentation",
        "freq_masks": 2,
        "time_masks": 5,
        "freq_width": 27,
        "time_width": 0.05,
    })
    cfg.optim = OmegaConf.create({
        "_target_": "torch.optim.AdamW",
        "lr": lr,
        "betas": [0.9, 0.98],
        "weight_decay": 0.001,
    })
    cfg.scheduler = OmegaConf.create({
        "_target_": "nemo.core.optim.lr_scheduler.CosineAnnealing",
        "max_steps": epochs * 1000,
        "min_lr": lr * 0.01,
        "warmup_steps": 500,
        "warmup_ratio": None,
    })
    cfg.decoder = cfg.get("decoder", OmegaConf.create({}))
    cfg.decoder.fastemit_lambda = 0.001

    if use_lora:
        print("[2/5] LoRA mode: freezing encoder")
        for name, param in model.named_parameters():
            if name.startswith("encoder"):
                param.requires_grad = False
    else:
        print("[2/5] Full fine-tune mode")

    model.setup_training_data(train_data_config=cfg.train_ds)
    model.setup_validation_data(val_data_config=cfg.validation_ds)
    model.setup_optimization(optim_config=cfg.optim)

    # Trainer
    trainer = pl.Trainer(
        max_epochs=epochs,
        accelerator="gpu",
        devices=1,
        precision="16-mixed",
        default_root_dir=str(ckpt_dir),
        enable_checkpointing=True,
        callbacks=[
            ModelCheckpoint(
                dirpath=str(version_dir),
                monitor="val_wer",
                mode="min",
                save_top_k=3,
                save_last=True,
                filename="{epoch:02d}-{val_wer:.3f}",
            ),
            EarlyStopping(monitor="val_wer", patience=5, mode="min", verbose=True),
            LearningRateMonitor(logging_interval="step"),
        ],
        log_every_n_steps=10,
        gradient_clip_val=1.0,
    )

    # Train
    print(f"[3/5] Starting training: {epochs} epochs")
    json_log(0, {"event": "train_start", "epochs": epochs, "batch_size": batch_size, "lr": lr})
    try:
        trainer.fit(model)
    except Exception as exc:
        print(f"[ERROR] Training failed: {exc}")
        traceback.print_exc()
        json_log(0, {"event": "train_error", "error": str(exc)})
        return {"status": "error", "stage": "training", "error": str(exc)}

    # Validation
    print("[4/5] Running validation")
    val_results = trainer.validate(model)
    val_wer = val_results[0].get("val_wer", None) if val_results else None
    json_log(0, {"event": "validation_end", "val_wer": val_wer})

    # Test eval
    if test_manifest:
        print("[5/5] Running test evaluation")
        cfg.test_ds = OmegaConf.create({
            "manifest_filepath": test_manifest,
            "sample_rate": 16000,
            "batch_size": batch_size,
            "shuffle": False,
            "num_workers": 4,
            "max_duration": 25.0,
            "min_duration": 0.1,
        })
        model.setup_test_data(test_data_config=cfg.test_ds)
        test_results = trainer.test(model)
        json_log(0, {"event": "test_end", "test_results": test_results})
    else:
        print("[5/5] No test manifest; skipping test eval")

    # Save final
    final_ckpt = version_dir / "final_model.nemo"
    model.save_to(str(final_ckpt))
    config_path = version_dir / "model_config.yaml"
    OmegaConf.save(config=cfg, f=str(config_path))

    summary = {
        "exp_name": exp_name,
        "version_dir": str(version_dir),
        "base_model": BASE_MODEL,
        "epochs": epochs,
        "final_ckpt": str(final_ckpt),
        "val_wer": val_wer,
        "completed_at": datetime.now(timezone.utc).isoformat(),
    }
    (version_dir / "run_summary.json").write_text(json.dumps(summary, indent=2))
    
    print(f"\n[OK] Run complete. Outputs: {version_dir}")
    return summary


# ── Modal Entrypoint (Local CLI) ─────────────────────────────────────────────

@app.local_entrypoint()
def main(
    epochs: int = 10,
    batch_size: int = 8,
    exp_name: str = "wolof_nemotron35_asr_v1",
    skip_manifest: bool = False,
    skip_train: bool = False,
    use_lora: bool = False,
):
    """Orchestrate the full pipeline from local CLI."""
    
    if not skip_manifest:
        print("\n=== Phase 1: Create Manifests ===")
        manifest_result = create_manifests.remote(
            dataset_name=DATASET_NAME,
            output_dir=str(MANIFEST_DIR),
        )
        print(f"Manifests created: {manifest_result}")
    else:
        print("\n=== Phase 1: Skipped (using existing manifests) ===")
        manifest_result = {
            "manifests": {
                "train": str(MANIFEST_DIR / "wolof_nemo_manifest_train.jsonl"),
                "validation": str(MANIFEST_DIR / "wolof_nemo_manifest_validation.jsonl"),
                "test": str(MANIFEST_DIR / "wolof_nemo_manifest_test.jsonl"),
            }
        }

    if not skip_train:
        print("\n=== Phase 2: Fine-tune ASR ===")
        train_result = finetune_asr.remote(
            train_manifest=manifest_result["manifests"]["train"],
            val_manifest=manifest_result["manifests"]["validation"],
            test_manifest=manifest_result["manifests"].get("test"),
            epochs=epochs,
            batch_size=batch_size,
            exp_name=exp_name,
            use_lora=use_lora,
        )
        print(f"Training complete: {train_result}")
    else:
        print("\n=== Phase 2: Skipped (training disabled) ===")

    print("\n=== Pipeline Complete ===")
    print(f"Checkpoints saved to Modal volume: {VOLUME_NAME}")
    print(f"Mount path inside container: {MOUNT_PATH}")
    print("\nTo download checkpoints locally:")
    print(f"  modal volume get {VOLUME_NAME} output/nemo-asr/checkpoints .")
