"""YAATAL — Modal bake-off: pure-Transformer vs hybrid (Mamba2+attention) edge tool-router.

Trains the SAME synthetic tool-router data with the SAME base-agnostic LoRA trainer on
several bases, then for each: eval (tuned vs zero-shot scoreboard) -> GGUF Q4_K_M ->
llama-bench (CPU t/s, the edge proxy). Bases run in PARALLEL containers (one L4 each),
results aggregated into one comparison. Answers the hybrid question empirically:
task accuracy AND on-device size/latency, not assertion.

  Default set: Qwen2.5-1.5B (Transformer) vs granite-4.0-h-1b (hybrid, Apache-2.0)
               vs Falcon-H1-1.5B (hybrid). ~25 min/base, ~$0.4 each => a few $ total.

Run:
    modal run scripts/modal_bakeoff.py                 # full default set
    modal run scripts/modal_bakeoff.py --max-steps 90
    modal run scripts/modal_bakeoff.py --only granite-4.0-h-1b   # one base
Knobs: YAATAL_TR_GPU=A10G (default L4).

PERFORMANCE NOTE — hybrid training is slow here, on purpose (see also
output/yaatal-data-factory/reports/hybrid-training-kernels.md):
    The Mamba2 hybrids (granite-4.0-h-*, Falcon-H1) train at ~67 s/it on an L4 — ~75 min
    for 60 steps — because the fast SSM CUDA kernels (mamba-ssm + causal-conv1d) are NOT
    in this image, so HuggingFace falls back to a naive PyTorch SSM recurrence (20-40x
    slower). This is a TRAINING-TIME cost ONLY: llama.cpp implements Mamba2 in C++, so the
    exported GGUF and the llama-bench edge-latency numbers are unaffected and fair. The
    function timeout is 7200s so the fallback can finish.
    To make hybrid training fast (minutes, not ~75 min), add the kernels — but they need a
    CUDA-devel toolchain (nvcc) to compile, or prebuilt wheels matching the EXACT
    torch+CUDA+python ABI (here torch 2.8 / cu12x / cp311), e.g.:
        modal.Image.from_registry("nvidia/cuda:12.6.2-devel-ubuntu22.04", add_python="3.11")
          .pip_install("torch==2.8.0")
          .pip_install("causal-conv1d>=1.4.0", "mamba-ssm>=2.2.2")   # compiles against nvcc
    Deferred until a hybrid is actually chosen: it changes wall-clock only, not the result,
    so it's not worth the wheel/ABI-matching risk for a one-off comparison.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

import modal

GPU = os.environ.get("YAATAL_TR_GPU", "L4")
VOLUME_NAME = "yaatal-bakeoff-out"
SCRIPTS = ("train_tool_router_hf.py", "eval_tool_router.py", "export_gguf.py")
LOCAL_DATA = "output/yaatal-data-factory/bobo-tool"

# Current run = the 350M floor probe (see docs/SALM-ON-GRANITE-350M.md §6.D): can the aggressive-edge
# 350M hold slot fidelity vs the qualified 1B, BOTH trained on the v2 dataset (6,022 rows) so the
# comparison is same-data. data="v2" routes to the converted market_intent files.
SPECS = [
    # 350m-v2 completed (scoreboard+gguf in volume); finishing the 1b from its saved LoRA.
    {"label": "granite-4.0-h-1b-v2", "base": "ibm-granite/granite-4.0-h-1b", "kind": "hyb·1b",
     "trc": False, "data": "v2", "skip_train": True},
]

app = modal.App("yaatal-bakeoff")

# Clean, Unsloth-free image: stable cu124 torch + a transformers new enough for the
# granite-4.0 hybrid and falcon_h1 archs, + a llama.cpp we build ourselves (quantize +
# cli + bench). torch first so deps resolve against it; llama.cpp late so pip stays cached.
image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install("git", "build-essential", "cmake", "curl", "libcurl4-openssl-dev")
    # Build llama.cpp FIRST (a C++ compile, independent of the python stack) so torch/
    # transformers tweaks don't recompile it. Targets: quantize (GGUF) + cli + bench (edge t/s).
    .run_commands(
        "git clone --depth 1 https://github.com/ggml-org/llama.cpp /opt/llama.cpp",
        "cmake -S /opt/llama.cpp -B /opt/llama.cpp/build -DGGML_CUDA=OFF -DLLAMA_CURL=OFF -DBUILD_SHARED_LIBS=OFF",
        "cmake --build /opt/llama.cpp/build -j 4 --target llama-quantize llama-cli llama-bench",
        "pip install -e /opt/llama.cpp/gguf-py",
    )
    # torch 2.8 has torch.float8_e8m0fnu, which transformers>=4.57 imports at load (torch 2.5.1
    # lacked it -> transformers+peft failed to import before training even started).
    .pip_install("torch==2.8.0")
    .pip_install("transformers>=4.57.2", "peft>=0.13", "trl>=0.12", "datasets>=3.0",
                 "accelerate>=0.34", "sentencepiece", "protobuf", "hf_transfer", "wandb")
    .env({"HF_HUB_ENABLE_HF_TRANSFER": "1", "TOKENIZERS_PARALLELISM": "false", "LLAMA_CPP_DIR": "/opt/llama.cpp"})
    .add_local_file(f"scripts/{SCRIPTS[0]}", f"/work/scripts/{SCRIPTS[0]}")
    .add_local_file(f"scripts/{SCRIPTS[1]}", f"/work/scripts/{SCRIPTS[1]}")
    .add_local_file(f"scripts/{SCRIPTS[2]}", f"/work/scripts/{SCRIPTS[2]}")
    .add_local_dir(LOCAL_DATA, "/work/bobo-tool")
    .add_local_dir("output/yaatal-edge-agent/v2-trainer", "/work/v2-trainer")
)

# Per-dataset file map; specs select with their "data" field (default = the BOBO bootstrap).
DATASETS = {
    "bobo": {"train": "/work/bobo-tool/synthetic_bootstrap_train.jsonl",
             "eval": "/work/bobo-tool/synthetic_bootstrap_val.jsonl",
             "lexicon": "/work/bobo-tool/slot_lexicon.json", "system": None},
    "v2": {"train": "/work/v2-trainer/train.jsonl",
           "eval": "/work/v2-trainer/val_sample.jsonl",
           "lexicon": "/work/v2-trainer/lexicon.json", "system": "/work/v2-trainer/system.txt"},
}
vol = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)


def _run(name: str, cmd: list[str]) -> int:
    print(f"\n=== [{name}] {' '.join(cmd[1:])} ===", flush=True)
    rc = subprocess.run(cmd, cwd="/work").returncode
    print(f"=== [{name}] exit={rc} ===", flush=True)
    return rc


def _bench(gguf: str) -> dict:
    """llama-bench on CPU (the edge proxy): prompt-processing + generation t/s."""
    try:
        out = subprocess.run(
            ["/opt/llama.cpp/build/bin/llama-bench", "-m", gguf, "-t", "4", "-p", "64", "-n", "32", "-r", "3"],
            cwd="/work", capture_output=True, text=True, timeout=600,
        ).stdout
    except Exception as exc:  # noqa: BLE001
        return {"error": str(exc)}
    res: dict = {"raw": out.strip().splitlines()[-6:]}
    for line in out.splitlines():
        m = re.search(r"\b(pp|tg)\d+\b.*?\|\s*([\d.]+)\s*±", line)
        if m:
            res[f"{m.group(1)}_tok_s"] = float(m.group(2))
    return res


@app.function(image=image, gpu=GPU, timeout=7200, volumes={"/work/output": vol})
def run_one(spec: dict) -> dict:
    label, base, trc = spec["label"], spec["base"], spec.get("trc", False)
    max_steps = int(spec.get("max_steps", 60))
    ds = DATASETS[spec.get("data", "bobo")]
    sys_flag = ["--system-file", ds["system"]] if ds["system"] else []
    # v2 rows are ~2-4x longer than the bobo bootstrap; at batch 4 the CE-loss logits over the
    # ~100k Granite vocab OOM a 24GB L4. batch 1 x accum 16 keeps the same effective batch.
    mem_flags = ["--batch", "1", "--grad-accum", "16", "--max-seq-len", "640"] \
        if spec.get("data") == "v2" else []
    root = f"/work/output/bakeoff/{label}"
    adapter, gguf_dir = f"{root}/lora", f"{root}/gguf"
    trc_flag = ["--trust-remote-code"] if trc else []
    r: dict = {"label": label, "base": base, "kind": spec.get("kind")}

    if spec.get("skip_train"):
        vol.reload()
        r["train_rc"] = "skipped"
        if not Path(adapter).exists():
            r["error"] = f"skip_train set but no adapter at {adapter}"
            return r
    else:
        r["train_rc"] = _run(f"{label}:train", [sys.executable, f"/work/scripts/{SCRIPTS[0]}",
            "--base", base, "--train", ds["train"],
            "--output-dir", adapter, "--max-steps", str(max_steps)] + trc_flag + sys_flag + mem_flags)
        vol.commit()
        if r["train_rc"] != 0:
            r["error"] = "train failed"
            return r

    r["eval_rc"] = _run(f"{label}:eval", [sys.executable, f"/work/scripts/{SCRIPTS[1]}",
        "--base", base, "--adapter", adapter,
        "--eval", ds["eval"],
        "--lexicon", ds["lexicon"],
        "--output-root", root, "--baseline"] + trc_flag + sys_flag)
    sb = Path(root) / "reports" / "tool_router_scoreboard.json"
    r["scoreboard"] = json.loads(sb.read_text()) if sb.exists() else None
    vol.commit()

    r["export_rc"] = _run(f"{label}:export", [sys.executable, f"/work/scripts/{SCRIPTS[2]}",
        "--base", base, "--adapter", adapter, "--out", gguf_dir, "--quant", "q4_k_m"] + trc_flag)
    meta = Path(gguf_dir) / "export_meta.json"
    if meta.exists():
        r["gguf_mb"] = json.loads(meta.read_text()).get("gguf_mb")
        gguf_file = next(Path(gguf_dir).glob("tool-router-*.gguf"), None)
        if gguf_file:
            r["bench"] = _bench(str(gguf_file))
    vol.commit()
    return r


def _pull(remote: str, local: str) -> None:
    Path(local).parent.mkdir(parents=True, exist_ok=True)
    try:
        subprocess.run(["modal", "volume", "get", VOLUME_NAME, remote, local, "--force"])
    except Exception as exc:  # noqa: BLE001
        print(f"[pull] {remote}: {exc}")


def _metrics(r: dict) -> dict:
    sb = (r.get("scoreboard") or {}).get("tuned") or {}
    b = r.get("bench") or {}
    return {"exact": sb.get("exact_match"), "slot_f1": sb.get("slot_f1"),
            "probes": sb.get("probe_pass"), "tool_acc": sb.get("tool_accuracy"),
            "json": sb.get("json_validity"), "gguf_mb": r.get("gguf_mb"),
            "pp_t/s": b.get("pp_tok_s"), "tg_t/s": b.get("tg_tok_s")}


@app.local_entrypoint()
def main(max_steps: int = 60, only: str = ""):
    specs = [s for s in SPECS if s["label"] == only] if only else SPECS
    specs = [{**s, "max_steps": max_steps} for s in specs]
    print(f"[bakeoff] gpu={GPU} max_steps={max_steps} bases={[s['label'] for s in specs]}")
    results = list(run_one.map(specs, return_exceptions=True))

    cols = ["exact", "slot_f1", "probes", "tool_acc", "json", "gguf_mb", "pp_t/s", "tg_t/s"]
    print("\n================ BAKE-OFF ================")
    hdr = f"{'label':<18}{'kind':<12}" + "".join(f"{c:>10}" for c in cols)
    print(hdr); print("-" * len(hdr))
    summary = []
    for spec, r in zip(specs, results):
        if not isinstance(r, dict):  # a container hard-failed (return_exceptions=True)
            r = {"label": spec["label"], "base": spec["base"], "kind": spec.get("kind"), "error": repr(r)}
        m = _metrics(r)
        summary.append({**{k: r.get(k) for k in ("label", "base", "kind")},
                        "metrics": m, "rcs": {k: r.get(k) for k in ("train_rc", "eval_rc", "export_rc")},
                        "error": r.get("error")})
        cells = "".join(f"{('' if m[c] is None else m[c]):>10}" for c in cols)
        print(f"{r.get('label',''):<18}{str(r.get('kind','')):<12}{cells}"
              + (f"   ERR:{r['error']}" if r.get("error") else ""))

    Path("output/yaatal-data-factory/reports").mkdir(parents=True, exist_ok=True)
    Path("output/yaatal-data-factory/reports/bakeoff_summary.json").write_text(
        json.dumps({"max_steps": max_steps, "results": summary}, indent=2), encoding="utf-8")
    print("\n[bakeoff] summary -> output/yaatal-data-factory/reports/bakeoff_summary.json")

    print("[pull] fetching scoreboards + adapters ...")
    _pull("bakeoff", "output/yaatal-data-factory/models/bakeoff")
    print(f"\n[pull] GGUFs persist in volume '{VOLUME_NAME}': "
          f"modal volume get {VOLUME_NAME} bakeoff <local> --force")
