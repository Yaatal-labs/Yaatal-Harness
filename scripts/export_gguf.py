# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "transformers>=4.57",
#   "torch>=2.4",
#   "peft>=0.13",
#   "accelerate>=0.34",
# ]
# ///
"""Export a trained tool-router LoRA to a phone-deployable GGUF — deterministically.

Merges the LoRA into the base (plain transformers+peft), then uses a PREBUILT llama.cpp
(convert_hf_to_gguf.py -> f16 GGUF, then llama-quantize -> Q4_K_M) at $LLAMA_CPP_DIR.
We do NOT use unsloth's bundled GGUF path: it clones bleeding-edge llama.cpp but drives
it with deprecated steps (`make clean`, `-DLLAMA_CURL`) that fail on current master.
Base-agnostic: works for Qwen, granite-4.0-h-*, Falcon-H1, etc. — anything llama.cpp
can convert. The Engine still validates every emitted tool-call (model proposes, disposes).

Run (on the Modal runner; llama.cpp prebuilt at /opt/llama.cpp):
    python scripts/export_gguf.py --adapter .../tool-router-lora --base Qwen/Qwen2.5-1.5B-Instruct \
        --out .../tool-router-gguf --quant q4_k_m
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description="Export tool-router LoRA -> GGUF via llama.cpp")
    ap.add_argument("--adapter", default="output/yaatal-data-factory/models/tool-router-lora")
    ap.add_argument("--base", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--out", default="output/yaatal-data-factory/models/tool-router-gguf")
    ap.add_argument("--quant", default="q4_k_m", help="q4_k_m (phone) | q5_k_m | q8_0 | f16")
    ap.add_argument("--llama-cpp-dir", default=os.environ.get("LLAMA_CPP_DIR", "/opt/llama.cpp"))
    ap.add_argument("--trust-remote-code", action="store_true")
    ap.add_argument("--keep-fp16", action="store_true", help="keep the intermediate f16 GGUF")
    args = ap.parse_args()

    import torch
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    merged = out / "merged-fp16"

    # 1) Merge LoRA into the base (CPU is fine for ~1.5B; avoids any CUDA dtype edge).
    print(f"[gguf] loading base {args.base} + adapter {args.adapter} ...", flush=True)
    base = AutoModelForCausalLM.from_pretrained(
        args.base, torch_dtype=torch.float16, trust_remote_code=args.trust_remote_code,
    )
    model = PeftModel.from_pretrained(base, args.adapter)
    model = model.merge_and_unload()
    tok = AutoTokenizer.from_pretrained(args.base, trust_remote_code=args.trust_remote_code)
    model.save_pretrained(str(merged), safe_serialization=True)
    tok.save_pretrained(str(merged))
    print(f"[gguf] merged fp16 -> {merged}", flush=True)

    # 2) HF -> GGUF f16, then 3) quantize -> requested type, via the prebuilt llama.cpp.
    llama = Path(args.llama_cpp_dir)
    convert = llama / "convert_hf_to_gguf.py"
    quantize = llama / "build" / "bin" / "llama-quantize"
    f16_gguf = out / "model-f16.gguf"
    q_gguf = out / f"tool-router-{args.quant}.gguf"

    subprocess.run([sys.executable, str(convert), str(merged),
                    "--outfile", str(f16_gguf), "--outtype", "f16"], check=True)
    print(f"[gguf] f16 gguf -> {f16_gguf}", flush=True)
    subprocess.run([str(quantize), str(f16_gguf), str(q_gguf), args.quant.upper()], check=True)
    print(f"[gguf] {args.quant} gguf -> {q_gguf}", flush=True)

    # Tidy: drop the merged HF dir (and the f16 unless kept) to save volume space.
    shutil.rmtree(merged, ignore_errors=True)
    if not args.keep_fp16:
        f16_gguf.unlink(missing_ok=True)

    size_mb = round(q_gguf.stat().st_size / 1e6, 1) if q_gguf.exists() else None
    (out / "export_meta.json").write_text(json.dumps({
        "adapter": args.adapter, "base": args.base, "quant": args.quant,
        "gguf": q_gguf.name, "gguf_mb": size_mb,
        "deploy": "llama.cpp / Engine Tier-1; pair with search_products.schema.json grammar for constrained decode",
        "smoke": f"llama-cli -m {q_gguf.name} -p '<chat-formatted utterance>' --grammar-file search_products.gbnf",
    }, indent=2), encoding="utf-8")
    print(f"[gguf] done: {q_gguf.name} ({size_mb} MB)")
    print("[gguf] deploy with llama.cpp + the schema grammar (constrained decode = 100% valid JSON)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
