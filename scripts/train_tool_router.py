# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "unsloth",
#   "trl>=0.12",
#   "transformers>=4.46",
#   "datasets>=3.0",
#   "wandb>=0.17",
# ]
# ///
"""Train the BOBO `search_products` edge tool-router — sub-2B, Nemotron-style agentic recipe.

Unsloth LoRA on a small instruct base (default Qwen2.5-1.5B-Instruct). Reads the
synthetic bootstrap (or real Dakar rows once collected) — same schema, so it's a
swap-and-retrain. **GPU runner only** (Modal / HF Jobs / NYIT); it will not run on
a CPU box. The model only *proposes* JSON; the Engine disposes.

Run (on a runner):
    python scripts/train_tool_router.py \
        --train output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_train.jsonl \
        --output-dir output/yaatal-data-factory/models/tool-router-lora --max-steps 60
"""
from __future__ import annotations

import argparse
import inspect
import json
from pathlib import Path

SYSTEM = (
    "You are BOBO's on-device tool router. The user speaks Wolof, French, or a "
    "Wolof-French-English mix. If the utterance is a product search, output ONLY a JSON "
    "object that calls search_products per the BOBO schema. Otherwise output exactly "
    '{"needs_tool": false, "tool": "none", "args": {}, "language_observed": "<code>", "confidence": <0..1>}. '
    "Output JSON only — no prose, no markdown."
)


def load_rows(path: str) -> list[dict]:
    rows = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description="Train search_products edge tool-router (Unsloth LoRA)")
    ap.add_argument("--base", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--train", default="output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_train.jsonl")
    ap.add_argument("--output-dir", default="output/yaatal-data-factory/models/tool-router-lora")
    ap.add_argument("--max-seq-len", type=int, default=1024)
    ap.add_argument("--max-steps", type=int, default=60)
    ap.add_argument("--epochs", type=float, default=None, help="overrides --max-steps if set")
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--lora-r", type=int, default=16)
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--run-id", default="tool-router-smoke")
    ap.add_argument("--wandb", action="store_true")
    args = ap.parse_args()

    from unsloth import FastLanguageModel  # import before trl/transformers so its patches apply
    from datasets import Dataset
    from trl import SFTConfig, SFTTrainer

    # 1.5B in bf16 fits a 22GB L4 comfortably — load_in_4bit=False sidesteps the broken
    # bitsandbytes/CUDA-13 wheel entirely (no quantization kernels are ever touched).
    model, tok = FastLanguageModel.from_pretrained(
        model_name=args.base, max_seq_length=args.max_seq_len, load_in_4bit=False, dtype=None,
    )
    model = FastLanguageModel.get_peft_model(
        model, r=args.lora_r, lora_alpha=args.lora_r, lora_dropout=0.0, bias="none",
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
        use_gradient_checkpointing="unsloth", random_state=7,
    )

    def to_text(row: dict) -> str:
        msgs = [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": row["utterance"]},
            {"role": "assistant", "content": json.dumps(row["target"], ensure_ascii=False)},
        ]
        return tok.apply_chat_template(msgs, tokenize=False)

    train_ds = Dataset.from_list([{"text": to_text(r)} for r in load_rows(args.train)])

    cfg = dict(
        per_device_train_batch_size=args.batch, gradient_accumulation_steps=2,
        warmup_ratio=0.05, learning_rate=args.lr, logging_steps=5, optim="adamw_torch",
        weight_decay=0.01, lr_scheduler_type="linear", seed=7, output_dir=args.output_dir,
        report_to=("wandb" if args.wandb else "none"), run_name=args.run_id,
    )
    if args.epochs:
        cfg["num_train_epochs"] = args.epochs
    else:
        cfg["max_steps"] = args.max_steps

    # Tolerate TRL API drift: max_seq_length was renamed to max_length, and SFTTrainer's
    # `tokenizer` kwarg to `processing_class`. Pick whatever the installed TRL exposes so
    # the same script runs unchanged across runners/versions.
    sft_fields = set(inspect.signature(SFTConfig).parameters)
    if "max_seq_length" in sft_fields:
        cfg["max_seq_length"] = args.max_seq_len
    elif "max_length" in sft_fields:
        cfg["max_length"] = args.max_seq_len
    if "dataset_text_field" in sft_fields:
        cfg["dataset_text_field"] = "text"

    trainer_kwargs = {"model": model, "train_dataset": train_ds, "args": SFTConfig(**cfg)}
    tok_kwarg = "processing_class" if "processing_class" in inspect.signature(SFTTrainer).parameters else "tokenizer"
    trainer_kwargs[tok_kwarg] = tok
    trainer = SFTTrainer(**trainer_kwargs)
    trainer.train()

    out = Path(args.output_dir)
    out.mkdir(parents=True, exist_ok=True)
    model.save_pretrained(str(out))
    tok.save_pretrained(str(out))
    (out / "train_meta.json").write_text(json.dumps({
        "base": args.base, "train_rows": len(train_ds), "lora_r": args.lora_r,
        "max_steps": args.max_steps, "epochs": args.epochs, "run_id": args.run_id,
    }, indent=2), encoding="utf-8")
    print(f"[train] LoRA adapter saved -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
