# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""YAATAL Data Factory — Phase 1 data registry snapshot.

Portable PEP 723 script (stdlib only). Emits a provenance-tagged registry of
every candidate source across the active model lanes. No downloads happen here
— this is the catalog the Source gate (Phase 2) chooses from.

Run:
    uv run --script scripts/yaatal_df_01_registry_snapshot.py --output-root output/yaatal-data-factory
"""
from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path


def hf(ds: str) -> str:
    return f"https://huggingface.co/datasets/{ds}"


REGISTRY = {
    "asr_acoustic": [
        {
            "id": "galsenai/wolof-audio-data",
            "url": hf("galsenai/wolof-audio-data"),
            "task_lane": "asr_acoustic",
            "use": "Wolof ASR adaptation for Nemotron 3.5 ASR; WER/CER eval",
            "known_shape": {"rows": 35075, "train": 28807, "test": 6268, "sample_rate_hz": 16000},
            "fields": ["audio", "sentence", "source"],
            "license_or_access_note": "Apache-2.0",
            "provenance": "huggingface_public_dataset",
            "allowed_use": "language adaptation, WER/CER eval, code-mix and market-speech grounding",
            "not_allowed_use": "do not mix synthetic/TTS audio into this real-audio ASR baseline; hold out test split",
            "review_required": False,
        },
    ],
    "translation_target_text": [
        {
            "id": "galsenai/centralized_wolof_french_translation_data",
            "url": hf("galsenai/centralized_wolof_french_translation_data"),
            "task_lane": "translation_target_text",
            "use": "FR-WO target-text bridge; NLLB/Wolof-NMT training",
            "known_shape": {"rows": 98345},
            "fields": ["wo", "fr", "source"],
            "license_or_access_note": "see dataset card",
            "provenance": "huggingface_public_dataset",
            "allowed_use": "target-text generation, translation training/eval, duplex text-tap supervision",
            "not_allowed_use": "generated target text must not become training truth without human review",
            "review_required": False,
        },
        {
            "id": "galsenai/french-wolof-translation",
            "url": hf("galsenai/french-wolof-translation"),
            "task_lane": "translation_target_text",
            "use": "additional FR-WO training/eval",
            "known_shape": {"rows": 17800},
            "fields": ["french", "wolof", "sources"],
            "license_or_access_note": "see dataset card",
            "provenance": "huggingface_public_dataset",
            "allowed_use": "translation training/eval",
            "not_allowed_use": "n/a",
            "review_required": False,
        },
    ],
    "retrieval_qa": [
        {
            "id": "masakhane/afriqa-gold-passages",
            "url": hf("masakhane/afriqa-gold-passages"),
            "task_lane": "retrieval_qa",
            "use": "multilingual QA/retrieval eval for BGE-M3",
            "known_shape": {"wolof": {"train": 503, "dev": 504, "test": 334}},
            "fields": ["question", "passage", "answer"],
            "license_or_access_note": "see dataset card",
            "provenance": "huggingface_public_dataset",
            "allowed_use": "Recall@10/nDCG@10 eval before any embedding fine-tune",
            "not_allowed_use": "do not shrink dimensions or fine-tune before measuring recall",
            "review_required": False,
        },
        {
            "id": "bobo-engine-local-docs",
            "url": "local",
            "task_lane": "retrieval_qa",
            "use": "BOBO/Engine docs + product/merchant content for grounding",
            "known_shape": {"rows": "user-provided"},
            "fields": ["doc_id", "text"],
            "license_or_access_note": "internal/private",
            "provenance": "local_private",
            "allowed_use": "retrieval/memory grounding once user confirms inclusion",
            "not_allowed_use": "do not include without user confirmation",
            "review_required": True,
        },
    ],
    "entity_normalization": [
        {
            "id": "masakhane/masakhaner2",
            "url": hf("masakhane/masakhaner2"),
            "task_lane": "entity_normalization",
            "use": "names, locations, dates, organization normalization tests",
            "known_shape": {"wolof": {"train": 4593, "validation": 656, "test": 1312}},
            "fields": ["tokens", "ner_tags"],
            "license_or_access_note": "see dataset card",
            "provenance": "huggingface_public_dataset",
            "allowed_use": "normalization/regression checks; text-only until paired",
            "not_allowed_use": "text-only until paired with reviewed audio or target JSON",
            "review_required": False,
        },
    ],
    "collection_candidates": [
        {
            "id": "youtube-public-web-audio",
            "url": "https://github.com/yt-dlp/yt-dlp",
            "task_lane": "collection_candidate",
            "use": "domain-realistic Wolof/French market speech",
            "known_shape": {"rows": "user-provided URLs"},
            "fields": ["audio", "source_url", "consent_status"],
            "license_or_access_note": "rights/consent review required",
            "provenance": "scraped_candidate",
            "allowed_use": "candidate only; usable after rights review, segmentation, transcription, language tag, approval",
            "not_allowed_use": "never enters training without review + transcript + language tag + user approval",
            "review_required": True,
        },
        {
            "id": "field-recordings",
            "url": "local",
            "task_lane": "collection_candidate",
            "use": "accents, code-mix, noise, names, prices, markets, routes",
            "known_shape": {"rows": "user-provided"},
            "fields": ["audio", "transcription", "speaker_id"],
            "license_or_access_note": "consent/provenance required",
            "provenance": "local_private",
            "allowed_use": "acoustic truth once consented, transcribed, language-tagged",
            "not_allowed_use": "do not push; do not use without consent + provenance",
            "review_required": True,
        },
        {
            "id": "bobo-command-recordings",
            "url": "local",
            "task_lane": "collection_candidate",
            "use": "real 24 kHz BOBO command audio + target_json labels (edge voice lane)",
            "known_shape": {"rows": "user-provided"},
            "fields": ["audio", "transcription", "target_json", "speaker_id", "paraphrase_family"],
            "license_or_access_note": "private",
            "provenance": "local_private",
            "allowed_use": "BOBO audio tool-router training once real audio provided",
            "not_allowed_use": "do not start paid training on text-only seed rows; do not push",
            "review_required": True,
        },
    ],
    "augmentation_teacher": [
        {
            "id": "soynade-research/Oolel-v0.1",
            "url": "https://huggingface.co/soynade-research/Oolel-v0.1",
            "task_lane": "augmentation_teacher",
            "use": "Wolof-aware teacher for surface-form paraphrase augmentation (locked target_json seeds)",
            "known_shape": {"params": "7.6B", "architecture": "qwen2"},
            "fields": ["generated_variants"],
            "license_or_access_note": "community open source, owner-confirmed (2026-06-10); no card tag — ATTRIBUTION to Soynade Research required",
            "provenance": "huggingface_public_model_outputs",
            "allowed_use": "targeted augmentation with locked target_json; outputs review-gated (synthetic_needs_review); credit Soynade Research in artifacts",
            "not_allowed_use": "never a label source; never invent labels",
            "review_required": False,
        },
    ],
    "synthetic_datasets": [
        {
            "id": "yaatal_market_intent_v0",
            "url": "local:output/yaatal-edge-agent/data",
            "task_lane": "synthetic_datasets",
            "use": "edge-agent market-intent training set (find_product + none negatives)",
            "known_shape": {"rows": 5000, "train": 3778, "validation": 718, "test": 504, "languages": ["wo", "wo-fr", "fr", "en"]},
            "fields": ["input", "target_json", "language_mix", "paraphrase_family", "split"],
            "license_or_access_note": "internal synthetic (template-generated by scripts/yaatal_edge_00)",
            "provenance": "synthetic_template_local",
            "allowed_use": "edge tool-router/intent SFT + eval; paraphrase-family split hygiene enforced",
            "not_allowed_use": "rows are synthetic_needs_review — not a native-language truth source until reviewed",
            "review_required": True,
        },
        {
            "id": "yaatal_market_intent_v0_augmented_oolel",
            "url": "local:output/yaatal-edge-agent/data_augmented_oolel",
            "task_lane": "synthetic_datasets",
            "use": "market_intent_v0 + guardrail-accepted rules/Oolel variants (wo/wo-fr rebalance, unit surfaces)",
            "known_shape": {"rows_v2": 6022, "base": 5000, "rules_variants": 193, "oolel_variants": 829,
                             "oolel_run": "oolel-best-prompt-full200 (autoresearch-optimized prompt, 69.8% guardrail acceptance)"},
            "fields": ["input", "target_json", "language_mix", "paraphrase_family", "split", "augmentation"],
            "license_or_access_note": "internal synthetic; Oolel-derived rows require ATTRIBUTION to Soynade Research (community open source)",
            "provenance": "synthetic_augmentation (rules + soynade-research/Oolel-v0.1 via scripts/yaatal_edge_05/06)",
            "allowed_use": "edge intent SFT; credit Soynade Research in model/data cards; native review still pending (quality gate)",
            "not_allowed_use": "rows remain synthetic_needs_review until native review; never treat as field truth",
            "review_required": True,
        },
        {
            "id": "bobo-tool-synthetic-bootstrap",
            "url": "local:output/yaatal-data-factory/bobo-tool",
            "task_lane": "synthetic_datasets",
            "use": "search_products tool-call bootstrap (208 rows) — de-risks train/eval/GGUF pipeline; SALM-backbone bake-off data",
            "known_shape": {"rows": 208, "train": 185, "validation": 23},
            "fields": ["utterance", "target", "lang_observed", "template_id", "split"],
            "license_or_access_note": "internal synthetic (templates from BoPlex Scenario A skeletons)",
            "provenance": "synthetic_template_local (scripts/yaatal_tool_aug.py)",
            "allowed_use": "pipeline de-risking, bake-offs, schema/grammar validation",
            "not_allowed_use": "NOT a substitute for real Dakar field data; needs_native_review on all rows",
            "review_required": True,
        },
    ],
    "style_context_sources": [
        {
            "id": "boplex-endtoend-scenarios",
            "url": "local:BoPlex_EndToEnd_Scenarios_April2026.md",
            "task_lane": "style_context_sources",
            "use": "voice-to-intent scenario narratives (market commerce, event coordination) — style/context + template skeletons",
            "known_shape": {"scenarios": 2},
            "fields": ["narrative_text"],
            "license_or_access_note": "internal/private (user-authored)",
            "provenance": "local_private",
            "allowed_use": "style/context grounding and utterance skeletons for synthetic generation",
            "not_allowed_use": "never a label source — labels come from locked target_json only",
            "review_required": False,
        },
    ],
    "tts_synthetic": [
        {
            "id": "xtts-v2-wolof-generated-audio",
            "url": "local:output/yaatal-data-factory/tts",
            "task_lane": "tts_synthetic",
            "use": "synthetic Wolof speech from galsenai/xTTS-v2-wolof over reviewed target text (BoPlex TTS lane, df_06/df_07)",
            "known_shape": {"rows": "see reports/boplex_tts_generation_report.json"},
            "fields": ["audio", "text", "voice_ref"],
            "license_or_access_note": "xTTS-v2-wolof model license gated/attribution [verify before commercial]",
            "provenance": "synthetic_tts_local",
            "allowed_use": "TTS-lane experiments; ALWAYS labeled synthetic speech",
            "not_allowed_use": "never mix into real-audio ASR baselines; never treat as field audio (hard rule)",
            "review_required": True,
        },
    ],
}


def main() -> int:
    ap = argparse.ArgumentParser(description="YAATAL data registry snapshot")
    ap.add_argument("--output-root", default="output/yaatal-data-factory")
    ap.add_argument("--limit", type=int, default=None, help="interface parity; unused")
    args = ap.parse_args()

    root = Path(args.output_root)
    (root / "registry").mkdir(parents=True, exist_ok=True)
    (root / "reports").mkdir(parents=True, exist_ok=True)
    now = datetime.now(timezone.utc).isoformat()

    snapshot = {"schema_version": "1.1", "generated_at": now, "lanes": REGISTRY}
    out = root / "registry" / "source_registry.json"
    out.write_text(json.dumps(snapshot, indent=2, ensure_ascii=False), encoding="utf-8")

    counts = {lane: len(items) for lane, items in REGISTRY.items()}
    review_required = sum(1 for items in REGISTRY.values() for s in items if s["review_required"])
    report = {
        "phase": "1-registry-snapshot",
        "generated_at": now,
        "registry_path": str(out),
        "source_counts_by_lane": counts,
        "total_sources": sum(counts.values()),
        "review_required_sources": review_required,
    }
    (root / "reports" / "registry-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")

    print(f"[registry] wrote {out}")
    print(f"[registry] lanes: {counts}; total {report['total_sources']}; "
          f"review-required {review_required}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
