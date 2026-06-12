# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""YAATAL — synthetic bootstrap pipeline for the search_products edge tool-router.

Template-based augmentation: native-authored utterance skeletons (from BoPlex
Scenario A) with controlled slot substitution. Because the utterance and its
target_json are built from the SAME chosen slots, labels are correct by
construction. Output is SYNTHETIC and needs_native_review — it bootstraps and
de-risks the train/eval/export pipeline; real Dakar field data remains the moat.

Run:
    uv run --script scripts/yaatal_tool_aug.py --output-root output/yaatal-data-factory --n 200 --seed 7
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
from datetime import datetime, timezone
from pathlib import Path

TEMPLATES = [
    {"id": "T1", "lang": "wo-fr", "colors": 2, "quality": True, "context": True,
     "text": "Bo, dama soxla {product} {quality}, bu {c1} ak bu {c2} — pour {context}. Fouma meun dem jënd ko ci {market} bi, prix bu {price} rekk."},
    {"id": "T2", "lang": "wo-fr", "colors": 1, "quality": True,
     "text": "Bo, fooy gis {product} {quality} bu {c1} ci {market} bi, prix bu {price}?"},
    {"id": "T3", "lang": "fr-wo", "colors": 1, "context": True,
     "text": "Bo, je cherche {product} {c1} pour {context}, prix bu {price}, ci {market}."},
    {"id": "T4", "lang": "wo-fr", "colors": 1,
     "text": "Bo, dama bëgg {product} bu {c1} ci {market}, prix bu {price} rekk."},
]
NEGATIVES = [
    {"text": "Bo, naka nga def?", "lang": "wo", "kind": "greeting"},
    {"text": "Bo, jërëjëf waay!", "lang": "wo", "kind": "thanks"},
    {"text": "Bo, ana sa boutique bi?", "lang": "wo", "kind": "directions"},
    {"text": "Bo, c'est combien le total pour nous?", "lang": "fr", "kind": "pricing"},
]


def qual_canon(term: str, qmap: dict) -> str | None:
    t = term.lower()
    if "holland" in t:
        return "premium_dutch_wax"
    if "baax" in t or "rafet" in t:
        return "good"
    return qmap.get(term)


def main() -> int:
    ap = argparse.ArgumentParser(description="Synthetic bootstrap for search_products router")
    ap.add_argument("--output-root", default="output/yaatal-data-factory")
    ap.add_argument("--lexicon", default=None, help="defaults to <output-root>/bobo-tool/slot_lexicon.json")
    ap.add_argument("--n", type=int, default=200, help="positive samples to generate")
    ap.add_argument("--neg-ratio", type=float, default=0.2)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()

    root = Path(args.output_root)
    bdir = root / "bobo-tool"
    bdir.mkdir(parents=True, exist_ok=True)
    lex_path = Path(args.lexicon) if args.lexicon else bdir / "slot_lexicon.json"
    lex = json.loads(lex_path.read_text(encoding="utf-8"))
    pools = lex["augmentation_pools"]
    cmap = lex["slot_maps"]["color"]
    mmap = lex["slot_maps"]["market"]
    ctxmap = lex["slot_maps"]["context"]
    qmap = lex["slot_maps"]["quality"]

    rng = random.Random(args.seed)
    rows: list[dict] = []
    seen: set[str] = set()

    def add(utterance: str, lang: str, target: dict, kind: str, template_id: str):
        if utterance in seen:
            return
        seen.add(utterance)
        rows.append({
            "id": f"syn-{len(rows):05d}",
            "utterance": utterance,
            "lang_observed": lang,
            "is_search_products": target["tool"] == "search_products",
            "target": target,
            "kind": kind,
            "template_id": template_id,
            "synthetic": True,
            "review_status": "needs_native_review",
            "source": "synthetic_template/BoPlex_Scenario_A",
        })

    tries = 0
    while sum(r["is_search_products"] for r in rows) < args.n and tries < args.n * 40:
        tries += 1
        t = rng.choice(TEMPLATES)
        product = rng.choice(pools["product"])
        quality = rng.choice(pools["quality"])
        market = rng.choice(pools["market"])
        price = rng.choice(pools["price"])
        context = rng.choice(pools["context"])
        cols = rng.sample(pools["color"], t["colors"])
        fields = {"product": product, "quality": quality, "market": market,
                  "price": price, "context": context, "c1": cols[0]}
        if t["colors"] == 2:
            fields["c2"] = cols[1]
        utt = t["text"].format(**fields)
        use_quality = bool(t.get("quality"))
        use_context = bool(t.get("context"))
        args_obj = {
            "query": (f"{product} Holland" if use_quality and "holland" in quality.lower() else product),
            "quality": qual_canon(quality, qmap) if use_quality else None,
            "color": [cmap.get(c, c) for c in cols],
            "market": mmap.get(market, market),
            "price_pref": "low",
            "context": ctxmap.get(context) if use_context else None,
            "quantity": None,
        }
        target = {"needs_tool": True, "tool": "search_products", "args": args_obj,
                  "language_observed": t["lang"], "confidence": 0.9}
        add(utt, t["lang"], target, "positive", t["id"])

    n_neg = int(sum(r["is_search_products"] for r in rows) * args.neg_ratio)
    for i in range(n_neg):
        neg = rng.choice(NEGATIVES)
        target = {"needs_tool": False, "tool": "none", "args": {},
                  "language_observed": neg["lang"], "confidence": 0.95}
        add(f'{neg["text"]} ', neg["lang"], target, "negative", "NEG")  # trailing space => dedupe variety
        add(neg["text"], neg["lang"], target, "negative", "NEG")

    for r in rows:
        b = int(hashlib.sha1(r["id"].encode()).hexdigest(), 16) % 100
        r["split"] = "validation" if b < 12 else "train"

    train = [r for r in rows if r["split"] == "train"]
    val = [r for r in rows if r["split"] == "validation"]
    for name, data in (("synthetic_bootstrap_train.jsonl", train), ("synthetic_bootstrap_val.jsonl", val)):
        with (bdir / name).open("w", encoding="utf-8") as f:
            for r in data:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")

    report = {
        "phase": "synthetic-bootstrap",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "total": len(rows),
        "positives": sum(r["is_search_products"] for r in rows),
        "negatives": sum(not r["is_search_products"] for r in rows),
        "splits": {"train": len(train), "validation": len(val)},
        "templates": [t["id"] for t in TEMPLATES],
        "caveats": [
            "SYNTHETIC, template-based — narrow phrasing diversity; Wolof slot values need native review",
            "labels correct-by-construction; purpose = de-risk train/eval/export pipeline before real data",
            "NOT a substitute for real Dakar field data (the moat)",
        ],
    }
    (root / "reports" / "synthetic-bootstrap-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"[aug] total={len(rows)} pos={report['positives']} neg={report['negatives']} "
          f"-> train={len(train)} val={len(val)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
