#!/usr/bin/env python3
"""Fail the live-evals job unless the row's result bundle says it is whole.

usage: live_evals_check.py <result dir> <row> <cap usd>

A live proof that did not happen, that stopped on the spend cap, or that is
missing a required measurement is not a pass. The harness writes `status.json`
(complete, missing, spend); this reads it, appends the summary to the job
summary and exits non-zero for anything but a complete run inside its cap.
"""
import json
import os
import sys


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__)
        return 64
    directory, row, cap_text = sys.argv[1], sys.argv[2], sys.argv[3]
    cap = float(cap_text)
    status_path = os.path.join(directory, "status.json")
    if not os.path.exists(status_path):
        print(f"::error::{row}: no status.json was written, so no measurement is present")
        return 1
    with open(status_path, encoding="utf-8") as f:
        status = json.load(f)
    spend = status.get("spend", {})
    spent = float(spend.get("spent_usd", 0.0))
    lines = [
        f"## {row}",
        f"- complete: {status.get('complete')}",
        f"- spend: ${spent:.4f} of a ${cap:.2f} cap over {spend.get('calls', 0)} calls"
        f" (stopped by the cap: {spend.get('stopped_by_cap')})",
    ]
    for m in status.get("missing", []):
        lines.append(f"- MISSING: {m}")
    summary = os.path.join(directory, "summary.md")
    out = os.environ.get("GITHUB_STEP_SUMMARY")
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
            if os.path.exists(summary):
                with open(summary, encoding="utf-8") as s:
                    f.write("\n" + s.read() + "\n")
    print("\n".join(lines))
    failed = False
    if not status.get("complete"):
        print(f"::error::{row}: a required measurement is missing; a partial live run is not a pass")
        failed = True
    if spent > cap + 1e-9:
        print(f"::error::{row}: spent ${spent:.4f}, over the ${cap:.2f} cap")
        failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
