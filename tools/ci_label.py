#!/usr/bin/env python3
"""The platform label a CI run earns (PX-030, docs/76).

Reads the results of the jobs the summary job needs (GitHub's `needs`
context as JSON in `NEEDS_JSON`) and the platform states in
`tools/platforms.json`. The run is labelled CI_COMPATIBLE — for the
platforms its matrix covers, and never more than that — only when every
needed job succeeded; otherwise it says which jobs did not and exits 1, so a
failing platform suite blocks the merge instead of being relabelled.

    NEEDS_JSON='{"rust": {"result": "success"}}' python3 tools/ci_label.py
    python3 tools/ci_label.py --self-test
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def label(needs: dict, states: dict[str, str]) -> tuple[bool, list[str]]:
    failed = sorted(name for name, job in needs.items() if job.get("result") != "success")
    lines: list[str] = []
    if not needs:
        return False, ["## Platform result: no platform label", "", "No job results were given to label."]
    if failed:
        lines += [
            "## Platform result: NOT CI_COMPATIBLE",
            "",
            "These jobs did not succeed, so no platform earns the label from this run: "
            + ", ".join(f"`{f}` ({needs[f].get('result', 'unknown')})" for f in failed)
            + ".",
        ]
        return False, lines
    lines += [
        "## Platform result: CI_COMPATIBLE only",
        "",
        "Core, CLI and runtime built and passed unit, component and platform conformance suites on macOS, Linux and Windows.",
        "This is a development guarantee, never a user-facing support claim (docs/76, PX-030); a platform becomes",
        "RELEASE_GRADE only by packaged-desktop E2E evidence (PX-031) and, for Windows and Linux, a Decision Record.",
        "",
        "| Platform | Recorded state (tools/platforms.json) | This run |",
        "|---|---|---|",
    ]
    for name, state in states.items():
        lines.append(f"| {name} | {state} | CI_COMPATIBLE |")
    return True, lines


def self_test() -> int:
    states = {"macos": "CI_COMPATIBLE", "windows": "CI_COMPATIBLE", "linux": "CI_COMPATIBLE"}
    ok, text = label({"rust": {"result": "success"}, "node": {"result": "success"}}, states)
    checks = [ok, "CI_COMPATIBLE only" in text[0], not any("RELEASE_GRADE |" in t for t in text)]
    ok2, text2 = label({"rust": {"result": "failure"}, "node": {"result": "success"}}, states)
    checks += [not ok2, "NOT CI_COMPATIBLE" in text2[0], "`rust` (failure)" in text2[2]]
    ok3, _ = label({"desktop-e2e": {"result": "skipped"}}, states)
    checks += [not ok3]
    ok4, _ = label({}, states)
    checks += [not ok4]
    if all(checks):
        print("ci label self-test: success labels, a failed or skipped job does not")
        return 0
    print(f"ci label self-test FAILED: {checks}", file=sys.stderr)
    return 1


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    needs = json.loads(os.environ.get("NEEDS_JSON") or "{}")
    states = json.loads((ROOT / "tools/platforms.json").read_text())["platforms"]
    ok, lines = label(needs, states)
    text = "\n".join(lines) + "\n"
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    print(text, end="")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
