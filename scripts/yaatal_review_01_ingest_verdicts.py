# /// script
# requires-python = ">=3.10"
# dependencies = ["psycopg[binary]>=3.1", "requests>=2.31"]
# ///
"""Ingest filled review CSVs: guardrail-check edits, write verdicts to Supabase.

Reads the reviewer's filled CSV (downloaded from the Sheet as CSV) plus the .rows.json
sidecar from the export step. For every fix and every "own phrasing", the Modal guardrail
endpoint re-validates the text against the locked label before anything lands. Writes
append-only review_events (+ human_variants for own phrasings), updates row statuses,
and prints a batch report. Rows whose edits fail the guardrail stay pending and are
listed for a follow-up mini-batch.

Env:
    YAATAL_SUPABASE_DB_URL   Postgres connection string (service role)
    YAATAL_GUARDRAIL_URL     defaults to the deployed Modal endpoint

Run:
    uv run --script scripts/yaatal_review_01_ingest_verdicts.py \
        --csv output/yaatal-review/batch-001-filled.csv \
        --sidecar output/yaatal-review/batch-001.rows.json \
        --batch batch-001 --reviewer awa
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import sys
from pathlib import Path

DEFAULT_GUARDRAIL = "https://mouhamedn96--yaatal-review-guardrail-validate.modal.run"
VERDICT_MAP = {"a": "approve", "c": "fix", "r": "reject"}


def guardrail_check(url: str, text: str, target_json: dict) -> tuple[bool, list]:
    import requests
    payload = {"rows": [{"id": "edit", "input": text, "target_json": target_json}]}
    r = requests.post(url, json=payload, timeout=30)
    r.raise_for_status()
    res = r.json()["results"][0]
    return res["ok"], res.get("errors", [])


def main() -> int:
    ap = argparse.ArgumentParser(description="Ingest reviewer verdicts into Supabase")
    ap.add_argument("--csv", required=True, help="filled CSV downloaded from the Sheet")
    ap.add_argument("--sidecar", required=True, help=".rows.json from the export step")
    ap.add_argument("--batch", required=True)
    ap.add_argument("--reviewer", required=True)
    ap.add_argument("--db-url", default=os.environ.get("YAATAL_SUPABASE_DB_URL"))
    ap.add_argument("--guardrail-url", default=os.environ.get("YAATAL_GUARDRAIL_URL", DEFAULT_GUARDRAIL))
    args = ap.parse_args()

    if not args.db_url:
        print("[ingest] set YAATAL_SUPABASE_DB_URL")
        return 1

    import psycopg

    sidecar = json.loads(Path(args.sidecar).read_text(encoding="utf-8"))
    stats = {"approve": 0, "fix": 0, "reject": 0, "human_variant": 0, "guardrail_bounced": 0, "skipped": 0}
    bounced: list[str] = []

    with Path(args.csv).open(encoding="utf-8-sig", newline="") as fh, \
         psycopg.connect(args.db_url) as conn, conn.cursor() as cur:
        for row in csv.DictReader(fh):
            num = (row.get("N°") or row.get("N") or "").strip()
            meta = sidecar.get(num)
            if not meta:
                continue
            row_id, target = meta["row_id"], meta["target_json"]
            verdict = VERDICT_MAP.get((next((v for k, v in row.items() if k and k.startswith("VERDICT")), "") or "").strip().lower()[:1])
            edited = (next((v for k, v in row.items() if k and k.startswith("CORRECTION")), "") or "").strip()
            own = (next((v for k, v in row.items() if k and k.startswith("TA PROPRE")), "") or "").strip()
            reasons = (next((v for k, v in row.items() if k and k.startswith("RAISON")), "") or "").strip()
            notes = (row.get("NOTES") or "").strip()

            # Own phrasing is gold regardless of the verdict on the original row.
            if own:
                ok, errors = guardrail_check(args.guardrail_url, own, target)
                cur.execute(
                    """insert into pipeline.human_variants
                       (seed_row_id, utterance, target_json, reviewer, guardrail_ok)
                       values (%s, %s, %s, %s, %s)""",
                    (row_id, own, json.dumps(target, ensure_ascii=False), args.reviewer, ok),
                )
                stats["human_variant"] += 1
                if not ok:
                    bounced.append(f"{row_id} (own phrasing): {errors}")

            if verdict is None:
                stats["skipped"] += 1
                continue

            g_ok, g_errors = (None, None)
            new_status = {"approve": "approved", "fix": "fixed", "reject": "rejected"}[verdict]
            if verdict == "fix":
                if not edited:
                    stats["skipped"] += 1
                    continue
                g_ok, g_errors = guardrail_check(args.guardrail_url, edited, target)
                if not g_ok:
                    stats["guardrail_bounced"] += 1
                    bounced.append(f"{row_id} (fix): {g_errors}")
                    new_status = None  # stays pending; goes into the follow-up mini-batch

            cur.execute(
                """insert into pipeline.review_events
                   (row_id, reviewer, verdict, edited_text, reject_reasons, guardrail_ok, guardrail_errors, notes)
                   values (%s, %s, %s, %s, %s, %s, %s, %s)""",
                (row_id, args.reviewer, verdict, edited or None,
                 [r.strip() for r in reasons.split("|") if r.strip()] or None,
                 g_ok, json.dumps(g_errors) if g_errors else None, notes or None),
            )
            if new_status:
                cur.execute("update pipeline.review_rows set status = %s where row_id = %s",
                            (new_status, row_id))
                stats[verdict] += 1

        cur.execute(
            """update pipeline.batches set status =
                 case when not exists (select 1 from pipeline.review_rows
                                       where batch_id = %s and status = 'pending')
                      then 'done' else 'in_review' end
               where batch_id = %s""",
            (args.batch, args.batch),
        )
        conn.commit()

    print(f"[ingest] batch={args.batch} reviewer={args.reviewer}")
    for k, v in stats.items():
        print(f"  {k}: {v}")
    if bounced:
        print("[ingest] guardrail bounced (still pending, re-batch these):")
        for b in bounced:
            print(f"  - {b}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
