#!/usr/bin/env python3
"""Fail the live-core job unless every requested row left a whole, passing record.

usage: live_core_check.py <results dir> <rows, comma separated> <cap usd>

The live legs of the Core qualifications (services/modbit-core/tests/live_core.rs)
each write `<row>-<leg>.json`. A live proof that did not happen, that failed a
named check, that was stopped by the spend cap, or that was not live at all is
not a pass:

- a requested row with no record at all fails (the leg never ran or crashed
  before it could write one);
- a record with `passed: false` fails (and its failed checks are printed);
- a record that says `stopped_by_cap` or `live: false` fails;
- total spend above the cap fails.

The summary is appended to the job summary.
"""
import glob
import json
import os
import sys


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__)
        return 64
    directory, rows_text, cap_text = sys.argv[1], sys.argv[2], sys.argv[3]
    rows = [r.strip() for r in rows_text.split(",") if r.strip()]
    cap = float(cap_text)
    records = []
    for path in sorted(glob.glob(os.path.join(directory, "*.json"))):
        try:
            with open(path, encoding="utf-8") as f:
                rec = json.load(f)
        except (OSError, ValueError) as e:
            print(f"::error::{path}: unreadable result record: {e}")
            return 1
        rec["_file"] = os.path.basename(path)
        records.append(rec)
    failed = False
    lines = ["## live-core results", "", "| row | leg | passed | calls | tokens in/out | cost USD |", "|---|---|---|---|---|---|"]
    spent = 0.0
    for rec in records:
        spent += float(rec.get("cost_usd", 0.0))
        lines.append(
            f"| {rec.get('row')} | {rec.get('leg')} | {rec.get('passed')} | {rec.get('model_calls', 0)} "
            f"| {rec.get('input_tokens', 0)}/{rec.get('output_tokens', 0)} | {float(rec.get('cost_usd', 0.0)):.4f} |"
        )
    lines.append("")
    for row in rows:
        mine = [r for r in records if r.get("row") == row]
        if not mine:
            print(f"::error::{row}: no result record was written, so nothing was proven")
            lines.append(f"- {row}: NO RECORD")
            failed = True
            continue
        for rec in mine:
            bad = [c for c in rec.get("checks", []) if not c.get("ok")]
            if not rec.get("passed"):
                print(f"::error::{row}/{rec.get('leg')}: the leg did not pass: {rec.get('failure') or bad}")
                lines.append(f"- {row}/{rec.get('leg')}: FAILED {rec.get('failure') or ''}")
                for c in bad:
                    lines.append(f"  - check `{c.get('name')}`: {c.get('detail')}")
                failed = True
            if rec.get("stopped_by_cap"):
                print(f"::error::{row}/{rec.get('leg')}: the spend cap stopped the leg; a partial run is not a pass")
                failed = True
            if not rec.get("live"):
                print(f"::error::{row}/{rec.get('leg')}: not a live record (a scripted stand-in)")
                failed = True
    lines.append(f"- total spend: ${spent:.4f} of a ${cap:.2f} cap")
    if spent > cap + 1e-9:
        print(f"::error::spent ${spent:.4f}, over the ${cap:.2f} cap")
        failed = True
    out = os.environ.get("GITHUB_STEP_SUMMARY")
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
