# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "transformers>=4.46",
#   "torch>=2.2",
#   "peft>=0.13",
#   "accelerate>=0.34",
# ]
# ///
"""Eval the BOBO search_products edge tool-router — the challenge scoreboard.

Measures (tuned adapter vs zero-shot base): raw JSON-validity, tool-accuracy,
slot-F1, exact-match, the lexicon probes (yomb/Holland/mariaje...), and latency.
Unconstrained generation on purpose — raw JSON-validity is a model-quality signal;
deployment adds grammar-constrained decode for 100% validity. GPU runner (CPU ok,
slow for a sub-2B base).

Run:
    python scripts/eval_tool_router.py --base Qwen/Qwen2.5-1.5B-Instruct \
        --adapter output/yaatal-data-factory/models/tool-router-lora \
        --eval output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_val.jsonl \
        --lexicon output/yaatal-data-factory/bobo-tool/slot_lexicon.json --baseline
"""
from __future__ import annotations

import argparse
import json
import re
import time
from pathlib import Path

SYSTEM = (
    "You are BOBO's on-device tool router. The user speaks Wolof, French, or a "
    "Wolof-French-English mix. If the utterance is a product search, output ONLY a JSON "
    "object that calls search_products per the BOBO schema. Otherwise output exactly "
    '{"needs_tool": false, "tool": "none", "args": {}, "language_observed": "<code>", "confidence": <0..1>}. '
    "Output JSON only — no prose, no markdown."
)
_BRACE = re.compile(r"\{.*\}", re.DOTALL)


def load_rows(path: str) -> list[dict]:
    return [json.loads(x) for x in Path(path).read_text(encoding="utf-8").splitlines() if x.strip()]


def extract_json(text: str) -> dict | None:
    m = _BRACE.search(text or "")
    if not m:
        return None
    try:
        obj = json.loads(m.group(0))
        return obj if isinstance(obj, dict) else None
    except Exception:  # noqa: BLE001
        return None


def slot_pairs(target: dict) -> set:
    # Schema-agnostic: BOBO tool-calls carry tool+args; market_intent carries intent+entities.
    pairs = {("tool", target.get("tool") if "tool" in target else target.get("intent"))}
    a = target.get("args") or target.get("entities") or {}
    for s, v in a.items():
        if isinstance(v, list):
            for x in v:
                pairs.add((s, str(x)))
        elif v is not None:
            pairs.add((s, str(v)))
    return pairs


def f1(pred: set, gold: set) -> float:
    if not pred and not gold:
        return 1.0
    tp = len(pred & gold)
    p = tp / len(pred) if pred else 0.0
    r = tp / len(gold) if gold else 0.0
    return 2 * p * r / (p + r) if (p + r) else 0.0


def build_model(base: str, adapter: str | None, trust_remote_code: bool = False):
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    tok = AutoTokenizer.from_pretrained(base, trust_remote_code=trust_remote_code)
    model = AutoModelForCausalLM.from_pretrained(
        base, torch_dtype=torch.float16, device_map="auto", trust_remote_code=trust_remote_code)
    if adapter:
        from peft import PeftModel
        model = PeftModel.from_pretrained(model, adapter).merge_and_unload()
    model.eval()
    return model, tok


def generate(model, tok, utterance: str, max_new: int = 256) -> tuple[str, float]:
    try:
        msgs = [{"role": "system", "content": SYSTEM}, {"role": "user", "content": utterance}]
        enc = tok.apply_chat_template(msgs, add_generation_prompt=True, return_tensors="pt", return_dict=True)
    except Exception:  # noqa: BLE001 — template without a system role: fold it into the user turn
        msgs = [{"role": "user", "content": SYSTEM + "\n\n" + utterance}]
        enc = tok.apply_chat_template(msgs, add_generation_prompt=True, return_tensors="pt", return_dict=True)
    enc = enc.to(model.device)  # BatchEncoding (newer transformers no longer returns a bare tensor)
    input_len = enc["input_ids"].shape[1]
    t0 = time.time()
    out = model.generate(**enc, max_new_tokens=max_new, do_sample=False, pad_token_id=tok.eos_token_id)
    dt = (time.time() - t0) * 1000.0
    return tok.decode(out[0][input_len:], skip_special_tokens=True), dt


def score(model, tok, rows: list[dict], probes: list[dict]) -> dict:
    n = len(rows)
    valid = tool_ok = exact = 0
    f1_sum = lat_sum = 0.0
    for r in rows:
        text, dt = generate(model, tok, r["utterance"])
        lat_sum += dt
        pred = extract_json(text)
        gold = r["target"]
        if pred is None:
            continue
        valid += 1
        gold_tool = gold.get("tool") if "tool" in gold else gold.get("intent")
        pred_tool = pred.get("tool") if "tool" in pred else pred.get("intent")
        if pred_tool == gold_tool:
            tool_ok += 1
        if pred == gold:
            exact += 1
        f1_sum += f1(slot_pairs(pred), slot_pairs(gold))
    probe_pass = 0
    for pr in probes:
        utt = f"Bo, dama bëgg wax bu {pr['token']} ci HLM bi, prix bu yomb rekk."
        text, _ = generate(model, tok, utt)
        pred = extract_json(text) or {}
        a = pred.get("args") or {}
        v = a.get(pr["expect_slot"])
        hit = (pr["expect_value"] in v) if isinstance(v, list) else (str(v) == str(pr["expect_value"]))
        probe_pass += int(bool(hit))
    return {
        "rows": n,
        "json_validity": round(valid / n, 3) if n else 0.0,
        "tool_accuracy": round(tool_ok / n, 3) if n else 0.0,
        "exact_match": round(exact / n, 3) if n else 0.0,
        "slot_f1": round(f1_sum / n, 3) if n else 0.0,
        "probe_pass": f"{probe_pass}/{len(probes)}",
        "avg_latency_ms": round(lat_sum / n, 1) if n else 0.0,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="Eval search_products tool-router")
    ap.add_argument("--base", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--adapter", default="output/yaatal-data-factory/models/tool-router-lora")
    ap.add_argument("--eval", default="output/yaatal-data-factory/bobo-tool/synthetic_bootstrap_val.jsonl")
    ap.add_argument("--lexicon", default="output/yaatal-data-factory/bobo-tool/slot_lexicon.json")
    ap.add_argument("--output-root", default="output/yaatal-data-factory")
    ap.add_argument("--baseline", action="store_true", help="also score the zero-shot base (no adapter)")
    ap.add_argument("--trust-remote-code", action="store_true")
    ap.add_argument("--system-file", default=None, help="override the SYSTEM prompt from a file")
    ap.add_argument("--max-rows", type=int, default=None)
    args = ap.parse_args()

    if args.system_file:
        global SYSTEM
        SYSTEM = Path(args.system_file).read_text(encoding="utf-8").strip()

    rows = load_rows(args.eval)
    if args.max_rows:
        rows = rows[: args.max_rows]
    probes = json.loads(Path(args.lexicon).read_text(encoding="utf-8")).get("eval_probes", [])

    model, tok = build_model(args.base, args.adapter, args.trust_remote_code)
    tuned = score(model, tok, rows, probes)
    result = {"tuned": tuned}
    if args.baseline:
        bmodel, btok = build_model(args.base, None, args.trust_remote_code)
        result["zero_shot_base"] = score(bmodel, btok, rows, probes)

    out = Path(args.output_root) / "reports" / "tool_router_scoreboard.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"base": args.base, "adapter": args.adapter, **result}, indent=2), encoding="utf-8")
    print("=== TOOL-ROUTER SCOREBOARD ===")
    for k, v in result.items():
        print(f"[{k}] {v}")
    print(f"[scoreboard] -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
