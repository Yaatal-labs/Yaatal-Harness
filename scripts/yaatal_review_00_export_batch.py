# /// script
# requires-python = ">=3.10"
# dependencies = ["psycopg[binary]>=3.1"]
# ///
"""Export a review batch from Supabase to a Sheets-ready CSV.

Pulls a batch from pipeline.review_rows and writes a CSV the coordinator imports into
Google Sheets (File > Import). Columns the reviewer touches are at the end; slot chips
are rendered as plain text so nobody ever sees JSON. A JSON sidecar keeps the row_id map
so ingestion is exact even if the Sheet gets re-sorted.

Env:
    YAATAL_SUPABASE_DB_URL   Postgres connection string (Supabase dashboard > Settings >
                             Database > Connection string, URI form, with password)

Run:
    uv run --script scripts/yaatal_review_00_export_batch.py --batch batch-001 \
        --out output/yaatal-review/batch-001.csv
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import sys
from pathlib import Path

VERDICT_HELP = "A = Approuver | C = Corriger (remplir CORRECTION) | R = Rejeter (remplir RAISON)"
REASON_CHIPS = "pas_naturel | mauvais_melange_langue | sens_change | etiquette_fausse | autre"


def main() -> int:
    ap = argparse.ArgumentParser(description="Export a Supabase review batch to CSV")
    ap.add_argument("--batch", required=True)
    ap.add_argument("--out", default=None, help="defaults to output/yaatal-review/<batch>.csv")
    ap.add_argument("--db-url", default=os.environ.get("YAATAL_SUPABASE_DB_URL"))
    args = ap.parse_args()

    if not args.db_url:
        print("[export] set YAATAL_SUPABASE_DB_URL (Supabase > Settings > Database > URI)")
        return 1

    import psycopg

    out = Path(args.out or f"output/yaatal-review/{args.batch}.csv")
    out.parent.mkdir(parents=True, exist_ok=True)

    with psycopg.connect(args.db_url) as conn, conn.cursor() as cur:
        cur.execute(
            """select row_id, utterance, target_json, language_mix
               from pipeline.review_rows
               where batch_id = %s and status = 'pending'
               order by row_id""",
            (args.batch,),
        )
        rows = cur.fetchall()
        cur.execute(
            "update pipeline.batches set status = 'in_review' where batch_id = %s", (args.batch,)
        )
        conn.commit()

    if not rows:
        print(f"[export] no pending rows in {args.batch}")
        return 0

    sidecar = {}
    with out.open("w", encoding="utf-8-sig", newline="") as fh:  # BOM so Sheets/Excel keep ë/ñ
        w = csv.writer(fh)
        w.writerow(["N°", "PHRASE", "PRODUIT", "MARCHÉ", "COULEURS", "NOMBRE", "OCCASION",
                    f"VERDICT ({VERDICT_HELP})", "CORRECTION (si C)",
                    "TA PROPRE FORMULATION (optionnel, en or !)",
                    f"RAISON (si R: {REASON_CHIPS})", "NOTES"])
        for i, (row_id, utterance, target, lang) in enumerate(rows, start=1):
            ent = (target or {}).get("entities", {})
            w.writerow([i, utterance, ent.get("product", ""), ent.get("market", ""),
                        ", ".join(ent.get("colors") or []),
                        f"{ent.get('quantity', '')} {('mètre(s)' if ent.get('unit') == 'meter' else ent.get('unit') or '')}".strip(),
                        ent.get("occasion", ""), "", "", "", "", ""])
            sidecar[str(i)] = {"row_id": row_id, "language_mix": lang, "target_json": target}

    side_path = out.with_suffix(".rows.json")
    side_path.write_text(json.dumps(sidecar, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[export] {len(rows)} rows -> {out}")
    print(f"[export] sidecar -> {side_path} (keep next to the CSV; ingestion needs it)")
    print("[export] import the CSV into Google Sheets (File > Import), share with the reviewer,")
    print("[export] then run yaatal_review_01_ingest_verdicts.py on the filled export.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
