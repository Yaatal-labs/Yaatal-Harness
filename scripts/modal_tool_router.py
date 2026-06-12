"""YAATAL — one-button Modal runner for the BOBO search_products edge tool-router.

Chains train -> eval(--baseline) -> export(GGUF) on a single GPU container, on the
synthetic bootstrap set (or real Dakar rows once collected — same schema, same call).
The three stage scripts are reused unchanged; this file only provisions the GPU,
ships the data in, and pulls the artifacts (scoreboard + LoRA + GGUF) back out.

  COST: a 1.5B LoRA smoke is ~20-30 min on an L4 (~$0.80/hr) => well under $1/run.
        The $29 Modal credit covers dozens of full train->eval->export iterations.

Prereqs (already true in this repo): `modal` installed + `~/.modal.toml` authed,
and output/yaatal-data-factory/bobo-tool/*.jsonl present.

Run (the one button):
    modal run scripts/modal_tool_router.py
    modal run scripts/modal_tool_router.py --max-steps 90        # train longer
    modal run scripts/modal_tool_router.py --skip-export         # train+eval only (fastest)

Knobs via env (read at launch, before `modal run`):
    YAATAL_TR_GPU=A10G        # default L4; also A100, A100-80GB, T4, H100
    YAATAL_TR_WANDB=1         # log to W&B (requires a Modal secret named "wandb")

Artifacts land back in output/yaatal-data-factory/{reports,models}/ and persist in
the Modal Volume "yaatal-tool-router-out" (so you can re-pull or rerun a single stage).
The Engine still validates every emitted tool-call — model proposes, Engine disposes.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import modal

# --- launch-time knobs (resolved locally where `modal run` parses this file) ----------
GPU = os.environ.get("YAATAL_TR_GPU", "L4")
WANDB = os.environ.get("YAATAL_TR_WANDB") == "1"
VOLUME_NAME = "yaatal-tool-router-out"
APP_NAME = "yaatal-tool-router"

# Stage scripts + data shipped into the image (these exist in the repo).
SCRIPTS = ("train_tool_router.py", "eval_tool_router.py", "export_gguf.py")
LOCAL_DATA = "output/yaatal-data-factory/bobo-tool"

app = modal.App(APP_NAME)

# The image OWNS the dependency stack (the scripts' PEP-723 headers are inert here).
# torch is installed first so unsloth detects CUDA; unsloth then pins a compatible
# transformers/trl/peft/datasets/bitsandbytes stack. build tools are for the GGUF
# step (unsloth compiles llama.cpp at export time). If a future unsloth needs a
# different torch, pin it here — versions are captured in train_meta.json / W&B.
image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install("git", "build-essential", "cmake", "curl")
    .pip_install("torch==2.5.1")
    .pip_install("unsloth", "trl", "datasets", "accelerate")
    .pip_install("wandb", "hf_transfer", "sentencepiece", "protobuf")
    # Late layers (keep the expensive torch/unsloth pip layers cached). We build llama.cpp
    # ourselves and convert GGUF deterministically — unsloth_zoo's bundled GGUF path clones
    # bleeding-edge llama.cpp but drives it with deprecated steps (`make clean`, `-DLLAMA_CURL`)
    # that fail against current master. The prebuilt llama-quantize + convert_hf_to_gguf.py
    # live at /opt/llama.cpp; export_gguf.py shells out to them (no unsloth in the export path).
    .apt_install("libcurl4-openssl-dev")
    .run_commands(
        "git clone --depth 1 https://github.com/ggml-org/llama.cpp /opt/llama.cpp",
        "cmake -S /opt/llama.cpp -B /opt/llama.cpp/build -DGGML_CUDA=OFF -DLLAMA_CURL=OFF -DBUILD_SHARED_LIBS=OFF",
        "cmake --build /opt/llama.cpp/build -j 4 --target llama-quantize",
        "pip install -e /opt/llama.cpp/gguf-py",
    )
    .env({"HF_HUB_ENABLE_HF_TRANSFER": "1", "TOKENIZERS_PARALLELISM": "false", "LLAMA_CPP_DIR": "/opt/llama.cpp"})
    .add_local_file(f"scripts/{SCRIPTS[0]}", f"/work/scripts/{SCRIPTS[0]}")
    .add_local_file(f"scripts/{SCRIPTS[1]}", f"/work/scripts/{SCRIPTS[1]}")
    .add_local_file(f"scripts/{SCRIPTS[2]}", f"/work/scripts/{SCRIPTS[2]}")
    .add_local_dir(LOCAL_DATA, "/work/bobo-tool")
)

vol = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)
# Only require the "wandb" secret to exist when W&B is actually requested.
_secrets = [modal.Secret.from_name("wandb")] if WANDB else []


def _stage(name: str, cmd: list[str]) -> int:
    """Run one stage script, streaming its logs; return its exit code."""
    print(f"\n=== [{name}] {' '.join(cmd[1:])} ===", flush=True)
    rc = subprocess.run(cmd, cwd="/work").returncode
    print(f"=== [{name}] exit={rc} ===", flush=True)
    return rc


@app.function(image=image, gpu=GPU, timeout=3600, volumes={"/work/output": vol}, secrets=_secrets)
def run_pipeline(max_steps: int = 60, skip_train: bool = False, skip_eval: bool = False,
                 skip_export: bool = False, base: str = "Qwen/Qwen2.5-1.5B-Instruct",
                 wandb: bool = False) -> dict:
    import json

    if not wandb:
        os.environ["WANDB_MODE"] = "disabled"
    py = sys.executable
    adapter = "/work/output/models/tool-router-lora"
    gguf = "/work/output/models/tool-router-gguf"
    result: dict = {"gpu": GPU, "base": base, "max_steps": max_steps}

    # Stages are independent + idempotent: skipping train/eval reuses the adapter a
    # prior run committed to the volume (e.g. re-export the GGUF without re-paying to train).
    if skip_train or skip_eval:
        vol.reload()

    # 1) TRAIN — Unsloth LoRA on the synthetic bootstrap (correct-by-construction labels).
    if not skip_train:
        train_cmd = [py, f"/work/scripts/{SCRIPTS[0]}",
                     "--base", base,
                     "--train", "/work/bobo-tool/synthetic_bootstrap_train.jsonl",
                     "--output-dir", adapter,
                     "--max-steps", str(max_steps)] + (["--wandb"] if wandb else [])
        result["train_rc"] = _stage("train", train_cmd)
        vol.commit()
        if result["train_rc"] != 0:
            result["error"] = "train failed; nothing to eval/export"
            return result
    else:
        result["train_rc"] = "skipped"
        if not Path(adapter).exists():
            result["error"] = f"skip_train set but no adapter at {adapter} — run train first"
            return result

    # 2) EVAL — tuned adapter vs zero-shot base. Unconstrained gen => raw JSON-validity
    #    is a real model-quality signal (deployment adds grammar-constrained decode).
    if not skip_eval:
        eval_cmd = [py, f"/work/scripts/{SCRIPTS[1]}",
                    "--base", base,
                    "--adapter", adapter,
                    "--eval", "/work/bobo-tool/synthetic_bootstrap_val.jsonl",
                    "--lexicon", "/work/bobo-tool/slot_lexicon.json",
                    "--output-root", "/work/output",
                    "--baseline"]
        result["eval_rc"] = _stage("eval", eval_cmd)
        vol.commit()
    else:
        result["eval_rc"] = "skipped"

    # 3) EXPORT — merge LoRA -> GGUF Q4_K_M (phone artifact). Non-fatal: a flaky
    #    llama.cpp build must not cost you the scoreboard you already paid for.
    if not skip_export:
        export_cmd = [py, f"/work/scripts/{SCRIPTS[2]}",
                      "--adapter", adapter, "--out", gguf, "--quant", "q4_k_m"]
        result["export_rc"] = _stage("export", export_cmd)
        vol.commit()
    else:
        result["export_rc"] = "skipped"

    sb = Path("/work/output/reports/tool_router_scoreboard.json")
    result["scoreboard"] = json.loads(sb.read_text()) if sb.exists() else None
    return result


def _pull(remote: str, local: str) -> bool:
    """Best-effort `modal volume get` of one path; print the manual command on failure."""
    Path(local).parent.mkdir(parents=True, exist_ok=True)
    try:
        rc = subprocess.run(["modal", "volume", "get", VOLUME_NAME, remote, local, "--force"]).returncode
        return rc == 0
    except Exception as exc:  # noqa: BLE001
        print(f"[pull] could not fetch {remote}: {exc}")
        return False


@app.local_entrypoint()
def main(max_steps: int = 60, skip_train: bool = False, skip_eval: bool = False,
         skip_export: bool = False, base: str = "Qwen/Qwen2.5-1.5B-Instruct"):
    print(f"[modal] tool-router pipeline | gpu={GPU} max_steps={max_steps} "
          f"train={not skip_train} eval={not skip_eval} export={not skip_export} wandb={WANDB}")
    r = run_pipeline.remote(max_steps=max_steps, skip_train=skip_train, skip_eval=skip_eval,
                            skip_export=skip_export, base=base, wandb=WANDB)

    print("\n================ RESULT ================")
    print(f"  gpu={r.get('gpu')} base={r.get('base')} max_steps={r.get('max_steps')}")
    print(f"  train_rc={r.get('train_rc')} eval_rc={r.get('eval_rc')} export_rc={r.get('export_rc')}")
    sb = r.get("scoreboard")
    if sb:
        print("\n  --- SCOREBOARD (tuned vs zero-shot) ---")
        for k in ("tuned", "zero_shot_base"):
            if k in sb:
                print(f"  [{k}] {sb[k]}")
    if r.get("error"):
        print(f"  ERROR: {r['error']}")

    # Auto-pull the small artifacts (reports + LoRA adapter). The ~1GB GGUF is
    # opt-in — grab it with the printed command when you're ready to deploy.
    print("\n[pull] fetching scoreboard + LoRA adapter ...")
    _pull("reports", "output/yaatal-data-factory/reports")
    _pull("models/tool-router-lora", "output/yaatal-data-factory/models/tool-router-lora")
    print(
        "\n[pull] GGUF (phone artifact) — fetch when ready:\n"
        f"    modal volume get {VOLUME_NAME} models/tool-router-gguf "
        "output/yaatal-data-factory/models/tool-router-gguf --force"
    )
    print("\n[done] adapter+scoreboard local; artifacts also persist in volume "
          f"'{VOLUME_NAME}'.")
