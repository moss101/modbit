#!/usr/bin/env python3
"""Platform support claims (PX-030, docs/76 "Platform states").

CI compatibility is a development guarantee, never a user-facing support
claim. This scan fails when documentation, the desktop app or the CLI
presents a platform that has not earned RELEASE_GRADE as supported:

- the earned states live in `tools/platforms.json`; RELEASE_GRADE there is
  itself refused until PX-031 (platform promotion by packaged E2E) is
  COMPLETE in `graph/project-graph.json`;
- a sentence that names a platform and asserts support in the present tense
  ("Windows is supported", "supported on Linux", "Linux support",
  "release-grade on Windows", "production-ready on macOS") is a claim;
- a sentence that qualifies it ("not", "never", "until", "only", "target",
  "CI_COMPATIBLE", "promoted", …) is a definition of the rule, not a claim.

Run from the repository root:

    python3 tools/support_claims.py            # scan; exit 1 on any claim
    python3 tools/support_claims.py --self-test
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PLATFORMS = {
    "macos": re.compile(r"\b(?:macOS|Mac ?OS(?: X)?|OS X)\b", re.I),
    "windows": re.compile(r"\bWindows\b"),
    "linux": re.compile(r"\bLinux\b"),
}

# Present-tense support assertions about a platform (either order).
CLAIMS = [
    re.compile(r"\b{p}\b[^.;:\n|]{{0,40}}\b(?:is|are)\s+(?:now\s+|fully\s+|officially\s+)?(?:supported|release[- ]grade|production[- ]ready)\b", re.I),
    re.compile(r"\b(?:supported|release[- ]grade|production[- ]ready)\s+(?:on|for)\s+(?:[\w/ ,]{{0,30}}\b)?{p}\b", re.I),
    re.compile(r"\b(?:full|official)?\s*{p}\s+support\b", re.I),
    re.compile(r"\bsupports?\s+{p}\b", re.I),
]

QUALIFIERS = re.compile(
    r"\b(?:not|never|no|until|unless|only|target|targets|CI_COMPATIBLE|CI[- ]compatible|UNSUPPORTED|"
    r"unsupported|promot\w*|stay|stays|remain|remains|after|once|when|if|earned|evidence)\b",
    re.I,
)

TEXT_SUFFIXES = {".md", ".rs", ".ts", ".tsx", ".html", ".txt"}


def scanned_files(root: Path) -> list[Path]:
    """Documentation and the clients' own text (docs/76: docs, the app, the CLI)."""
    out: list[Path] = []
    for pattern in [
        "README.md",
        "docs/*.md",
        "apps/*/README.md",
        "packages/*/README.md",
        "apps/cli/src/**/*",
        "apps/desktop/src/**/*",
        "apps/desktop/index.html",
        "packages/vscode-adapter/src/**/*",
    ]:
        for p in root.glob(pattern):
            if p.is_file() and p.suffix in TEXT_SUFFIXES and "/gen/" not in p.as_posix():
                out.append(p)
    return sorted(set(out))


def sentences(text: str) -> list[str]:
    return [" ".join(s.split()) for s in re.split(r"(?<=[.!?])\s+|\n\s*\n|\|", text) if s.strip()]


def claims_in(text: str, earned: set[str]) -> list[tuple[str, str]]:
    """(platform, sentence) for every unqualified support claim about a
    platform that has not earned RELEASE_GRADE."""
    found = []
    for sent in sentences(text):
        if QUALIFIERS.search(sent):
            continue
        for name, pat in PLATFORMS.items():
            if name in earned or not pat.search(sent):
                continue
            for c in CLAIMS:
                if re.search(c.pattern.format(p=pat.pattern), sent, re.I):
                    found.append((name, sent))
                    break
    return found


def platform_states(root: Path) -> dict[str, str]:
    data = json.loads((root / "tools/platforms.json").read_text())
    return data["platforms"]


def promotion_complete(root: Path) -> bool:
    graph = json.loads((root / "graph/project-graph.json").read_text())
    nodes = graph.get("nodes", graph)
    items = nodes.values() if isinstance(nodes, dict) else nodes
    return any(n.get("id") == "PX-031" and n.get("status") == "COMPLETE" for n in items if isinstance(n, dict))


def scan(root: Path) -> list[str]:
    problems: list[str] = []
    states = platform_states(root)
    for name, state in states.items():
        if state not in {"RELEASE_GRADE", "CI_COMPATIBLE", "UNSUPPORTED"}:
            problems.append(f"tools/platforms.json: {name} has unknown state {state!r}")
    earned = {n for n, s in states.items() if s == "RELEASE_GRADE"}
    if earned and not promotion_complete(root):
        problems.append(
            f"tools/platforms.json: {sorted(earned)} marked RELEASE_GRADE but PX-031 (platform promotion) is not COMPLETE"
        )
        earned = set()
    for f in scanned_files(root):
        for name, sent in claims_in(f.read_text(errors="ignore"), earned):
            problems.append(f"{f.relative_to(root)}: presents {name} ({states.get(name, 'UNSUPPORTED')}) as supported: {sent[:200]!r}")
    return problems


def self_test() -> int:
    bad = [
        "Windows is supported.",
        "Modbit is fully supported on Linux.",
        "Linux support ships with this release.",
        "macOS is release-grade.",
        "The desktop app supports Windows and Linux.",
        "Release-grade on Windows as of today.",
    ]
    good = [
        "Windows and Linux are CI_COMPATIBLE: a development guarantee, never a user-facing support claim.",
        "macOS reaches RELEASE_GRADE by the packaged desktop E2E catalog.",
        "Windows stays CI_COMPATIBLE until its own packaged E2E passes.",
        "Windows is not supported yet.",
        "The broker kills the process tree on Windows.",
        "A RELEASE_GRADE target for Alpha.",
    ]
    failures = [s for s in bad if not claims_in(s, set())] + [s for s in good if claims_in(s, set())]
    if claims_in("Windows is supported.", {"windows"}):
        failures.append("an earned platform may be described as supported")
    for f in failures:
        print(f"self-test: misjudged {f!r}", file=sys.stderr)
    print(f"support claims self-test: {len(bad)} claims caught, {len(good)} definitions passed" if not failures else "support claims self-test FAILED")
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    problems = scan(ROOT)
    for p in problems:
        print(p, file=sys.stderr)
    states = platform_states(ROOT)
    summary = ", ".join(f"{k} {v}" for k, v in states.items())
    if problems:
        print(f"support claims: {len(problems)} problem(s); platform states: {summary}", file=sys.stderr)
        return 1
    print(f"support claims: {len(scanned_files(ROOT))} files scanned, no platform presented beyond its state ({summary})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
