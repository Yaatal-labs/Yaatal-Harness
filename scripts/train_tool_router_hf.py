# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "transformers>=4.57",
#   "torch>=2.4",
#   "peft>=0.13",
#   "trl>=0.12",
#   "datasets>=3.0",
#   "accelerate>=0.34",
#   "wandb>=0.17",
# ]
# ///
"""Base-agnostic LoRA trainer for the BOBO `search_products` edge tool-router.

Plain transformers+peft+trl (NO Unsloth) so the SAME script trains any HF causal LM —
pure-Transformer (Qwen2.5-1.5B) or hybrid Mamba2+attention (granite-4.0-h-1b,
Falcon-H1-1.5B). That portability is the point: a fair bake-off uses one trainer, one
hyperparam set, and only swaps --base. LoRA targets `all-linear`, so it adapts the
linear projections (incl. the SSM in/out projections) regardless of architecture; the
state-space conv/state params stay frozen, which is fine for SFT. The model only
*proposes* JSON; the Engine disposes. GPU runner only.

Run (on a runner):
    python scripts/train_tool_router_hf.py --base ibm-granite/granite-4.0-h-1b \
        --train output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_train.jsonl \
        --output-dir output/yaatal-data-factory/models/bakeoff/granite-h-1b --max-steps 60
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
    return [json.loads(x) for x in Path(path).read_text(encoding="utf-8").splitlines() if x.strip()]


def main() -> int:
    ap = argparse.ArgumentParser(description="Base-agnostic LoRA tool-router trainer")
    ap.add_argument("--base", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--train", default="output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_train.jsonl")
    ap.add_argument("--output-dir", default="output/yaatal-data-factory/models/tool-router-lora")
    ap.add_argument("--max-seq-len", type=int, default=1024)
    ap.add_argument("--max-steps", type=int, default=60)
    ap.add_argument("--epochs", type=float, default=None, help="overrides --max-steps if set")
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--lora-r", type=int, default=16)
    ap.add_argument("--batch", type=int, default=4)
    ap.add_argument("--grad-accum", type=int, default=4)
    ap.add_argument("--trust-remote-code", action="store_true", help="for bases that ship custom code")
    ap.add_argument("--system-file", default=None, help="override the SYSTEM prompt from a file")
    ap.add_argument("--run-id", default="tool-router-hf")
    ap.add_argument("--wandb", action="store_true")
    args = ap.parse_args()

    if args.system_file:
        global SYSTEM
        SYSTEM = Path(args.system_file).read_text(encoding="utf-8").strip()

    import torch
    from datasets import Dataset
    from peft import LoraConfig, get_peft_model
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from trl import SFTConfig, SFTTrainer

    tok = AutoTokenizer.from_pretrained(args.base, trust_remote_code=args.trust_remote_code)
    if tok.pad_token is None:  # most base/instruct ckpts have no pad token -> reuse eos for batching
        tok.pad_token = tok.eos_token

    model = AutoModelForCausalLM.from_pretrained(
        args.base, torch_dtype=torch.bfloat16, trust_remote_code=args.trust_remote_code,
    )
    model.config.use_cache = False  # required with gradient checkpointing
    model.gradient_checkpointing_enable()
    model.enable_input_require_grads()  # needed for grad-checkpointing + LoRA

    # all-linear targets attention+MLP across Qwen / Granite-hybrid / Falcon-H1; but on Mamba
    # hybrids peft refuses to adapt the SSM mixer linears (conv1d/in_proj/out_proj/x_proj/dt_proj),
    # so exclude them -> all bases adapt attention+MLP only (also makes the comparison fairer).
    lora_kwargs = dict(r=args.lora_r, lora_alpha=args.lora_r, lora_dropout=0.0, bias="none",
                       task_type="CAUSAL_LM", target_modules="all-linear")
    if "exclude_modules" in inspect.signature(LoraConfig).parameters:
        lora_kwargs["exclude_modules"] = ["conv1d", "in_proj", "out_proj", "x_proj", "dt_proj"]
    peft_cfg = LoraConfig(**lora_kwargs)
    model = get_peft_model(model, peft_cfg)
    model.print_trainable_parameters()

    def to_text(row: dict) -> str:
        msgs = [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": row["utterance"]},
            {"role": "assistant", "content": json.dumps(row["target"], ensure_ascii=False)},
        ]
        try:
            return tok.apply_chat_template(msgs, tokenize=False)
        except Exception:  # noqa: BLE001 — some templates reject a system role; fold it into user
            msgs = [{"role": "user", "content": SYSTEM + "\n\n" + row["utterance"]},
                    {"role": "assistant", "content": json.dumps(row["target"], ensure_ascii=False)}]
            return tok.apply_chat_template(msgs, tokenize=False)

    train_ds = Dataset.from_list([{"text": to_text(r)} for r in load_rows(args.train)])

    cfg = dict(
        per_device_train_batch_size=args.batch, gradient_accumulation_steps=args.grad_accum,
        warmup_ratio=0.05, learning_rate=args.lr, logging_steps=5, optim="adamw_torch",
        weight_decay=0.01, lr_scheduler_type="linear", seed=7, bf16=True, output_dir=args.output_dir,
        report_to=("wandb" if args.wandb else "none"), run_name=args.run_id,
    )
    if args.epochs:
        cfg["num_train_epochs"] = args.epochs
    else:
        cfg["max_steps"] = args.max_steps

    # Tolerate TRL API drift (max_seq_length -> max_length; tokenizer -> processing_class).
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
        "max_steps": args.max_steps, "epochs": args.epochs, "batch": args.batch,
        "grad_accum": args.grad_accum, "trust_remote_code": args.trust_remote_code, "run_id": args.run_id,
    }, indent=2), encoding="utf-8")
    print(f"[train-hf] LoRA adapter saved -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
