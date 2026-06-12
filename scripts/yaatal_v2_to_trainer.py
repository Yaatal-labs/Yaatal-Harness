# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Convert the market_intent v2 dataset into the Modal trainer's row format.

Maps data_augmented_oolel_v2 rows ({input, target_json, ...}) to the trainer schema
({utterance, target, ...}), samples a fixed eval subset for speed, and writes the
market_intent system prompt + an empty-probes lexicon (the BOBO yomb/Holland probes
don't map to this schema; slot_f1/exact carry the comparison).

Run:
    uv run --script scripts/yaatal_v2_to_trainer.py
"""
from __future__ import annotations

import json
import random
from pathlib import Path

SRC = Path("output/yaatal-edge-agent/data_augmented_oolel_v2")
DST = Path("output/yaatal-edge-agent/v2-trainer")
EVAL_SAMPLE = 150

SYSTEM = (
    "You are YAATAL's on-device market-intent router. The user speaks Wolof, French, English, "
    "or a mix. Output ONLY a JSON object with this exact shape: "
    '{"domain": "...", "intent": "...", "needs_tool": true|false, "language": "...", '
    '"entities": {"product": ..., "market": ..., "colors": [...], "price_constraint": ..., '
    '"occasion": ..., "quantity": ..., "unit": ...}, "confidence": <0..1>}. '
    "If the utterance is not a product search, output intent \"none\" with needs_tool false and "
    "empty entities. Output JSON only — no prose, no markdown."
)


def convert(rows: list[dict]) -> list[dict]:
    return [{"utterance": r["input"], "target": r["target_json"],
             "lang_observed": r.get("language_mix"), "id": r.get("id")} for r in rows]


def read_jsonl(p: Path) -> list[dict]:
    return [json.loads(x) for x in p.read_text(encoding="utf-8").splitlines() if x.strip()]


def write_jsonl(p: Path, rows: list[dict]) -> None:
    with p.open("w", encoding="utf-8", newline="\n") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")


def main() -> int:
    DST.mkdir(parents=True, exist_ok=True)
    train = convert(read_jsonl(SRC / "train.jsonl"))
    val = convert(read_jsonl(SRC / "validation.jsonl"))
    random.Random(7).shuffle(val)
    val_sample = val[:EVAL_SAMPLE]

    write_jsonl(DST / "train.jsonl", train)
    write_jsonl(DST / "val_sample.jsonl", val_sample)
    (DST / "system.txt").write_text(SYSTEM, encoding="utf-8")
    (DST / "lexicon.json").write_text(json.dumps({"eval_probes": []}), encoding="utf-8")
    print(f"[v2-trainer] train={len(train)} val_sample={len(val_sample)} -> {DST}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
