#!/usr/bin/env python3
"""Regenerate MANIFEST.md and manifest.json for the Modbit dossier.

Usage: python3 tools/build_manifest.py
Runs from any cwd. Standard library only (Python 3.9+).
"""
import hashlib
import json
import os
import re
import sys
from datetime import date

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")
EDITION = "V3.3 EPR v1.1"
AUTHORITY_DATE = "2026-09-05"

SECTIONS = [
    (0, 9, "authority", "Authority and orientation"),
    (10, 29, "architecture", "Architecture and subsystems"),
    (30, 39, "implementation", "Implementation specifications"),
    (40, 49, "requirements", "Requirements, tasks and traceability"),
    (50, 69, "verification", "Verification and testing"),
    (70, 79, "delivery", "Delivery and operations"),
    (80, 97, "governance", "Agent process and governance"),
    (98, 99, "live-state", "Live state"),
]

# V3 (flat, colliding numbers) -> V3.1 (docs/, unique numbers). Kept here so the
# old->new map is regenerated with the manifest and never drifts.
RENAMES = {
    "00_MASTER_INDEX.md": "00_MASTER_INDEX.md",
    "00_START_HERE_FOR_BUILD_AGENTS.md": "01_START_HERE_FOR_BUILD_AGENTS.md",
    "01_AUTHORITY_AND_DECISIONS.md": "02_AUTHORITY_AND_DECISIONS.md",
    "23_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md": "03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md",
    "17_REQUIREMENT_BASIS_AND_LIMITS.md": "04_REQUIREMENT_BASIS_AND_LIMITS.md",
    "02_PRODUCT_PRD_AND_UX.md": "10_PRODUCT_PRD_AND_UX.md",
    "03_SYSTEM_ARCHITECTURE.md": "11_SYSTEM_ARCHITECTURE.md",
    "04_REPOSITORY_AND_MODULE_LAYOUT.md": "12_REPOSITORY_AND_MODULE_LAYOUT.md",
    "05_DOMAIN_MODEL_AND_STATE_MACHINES.md": "13_DOMAIN_MODEL_AND_STATE_MACHINES.md",
    "06_AGENT_RUNTIME_AND_ORCHESTRATION.md": "14_AGENT_RUNTIME_AND_ORCHESTRATION.md",
    "07_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md": "15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md",
    "08_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md": "16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md",
    "21_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md": "17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md",
    "09_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md": "18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md",
    "10_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md": "19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md",
    "11_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md": "20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md",
    "12_TERMINAL_EXECUTION_AND_SANDBOX.md": "21_TERMINAL_EXECUTION_AND_SANDBOX.md",
    "13_BROWSER_AND_COMPUTER_USE.md": "22_BROWSER_AND_COMPUTER_USE.md",
    "14_SECURITY_POLICY_EFFECT_LEDGER.md": "23_SECURITY_POLICY_EFFECT_LEDGER.md",
    "15_CLOUD_CONTROL_PLANE_AND_SYNC.md": "24_CLOUD_CONTROL_PLANE_AND_SYNC.md",
    "20_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md": "25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md",
    "19_SKILL_REGISTRY_AND_EVOLUTION.md": "26_SKILL_REGISTRY_AND_EVOLUTION.md",
    "17_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md": "30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md",
    "18_DATABASE_AND_STORAGE_SCHEMA.md": "31_DATABASE_AND_STORAGE_SCHEMA.md",
    "19_DESKTOP_FRONTEND_IMPLEMENTATION.md": "32_DESKTOP_FRONTEND_IMPLEMENTATION.md",
    "20_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md": "33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md",
    "21_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md": "34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md",
    "27_DEPENDENCY_AND_BINDING_DECISIONS.md": "35_DEPENDENCY_AND_BINDING_DECISIONS.md",
    "30_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md": "36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md",
    "28_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md": "37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md",
    "29_OLD_REPO_DONOR_MIGRATION_RULES.md": "37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md (merged; was a strict subset)",
    "18_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md": "40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md",
    "40_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md": "41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md",
    "35_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md": "42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md",
    "27_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md": "43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md",
    "32_REQUIREMENTS_TRACEABILITY_MATRIX.md": "44_REQUIREMENTS_TRACEABILITY_MATRIX.md",
    "56_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md": "45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md",
    "24_REQUIREMENT_COVERAGE_FREEZE_GATE.md": "46_REQUIREMENT_COVERAGE_FREEZE_GATE.md",
    "39_REQUIREMENT_COVERAGE_AUDIT_REPORT.md": "47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md",
    "22_FEATURE_DEPTH_CONTRACTS.md": "48_FEATURE_DEPTH_CONTRACTS.md",
    "22_TEST_STRATEGY_REAL_SYSTEM_GATES.md": "50_TEST_STRATEGY_REAL_SYSTEM_GATES.md",
    "23_E2E_ACCEPTANCE_TEST_CATALOG.md": "51_E2E_ACCEPTANCE_TEST_CATALOG.md",
    "24_SECURITY_THREAT_MODEL_AND_TESTS.md": "52_SECURITY_THREAT_MODEL_AND_TESTS.md",
    "25_PERFORMANCE_AND_BENCHMARK_PLAN.md": "53_PERFORMANCE_AND_BENCHMARK_PLAN.md",
    "49_FAULT_INJECTION_AND_RECOVERY_CATALOG.md": "54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md",
    "50_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md": "55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md",
    "38_TOOL_CAPABILITY_CONFORMANCE.md": "56_TOOL_CAPABILITY_CONFORMANCE.md",
    "36_SKILL_EVOLUTION_REAL_TESTS.md": "57_SKILL_EVOLUTION_REAL_TESTS.md",
    "37_MULTIMODAL_MEDIA_REAL_TESTS.md": "58_MULTIMODAL_MEDIA_REAL_TESTS.md",
    "33_RELEASE_ZERO_PROOF_SCENARIO.md": "59_RELEASE_ZERO_PROOF_SCENARIO.md",
    "42_RELEASE_ZERO_EXPANDED_PROOF.md": "60_RELEASE_ZERO_EXPANDED_PROOF.md",
    "26_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md": "70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md",
    "31_OPERATIONS_RUNBOOK.md": "71_OPERATIONS_RUNBOOK.md",
    "34_RISK_REGISTER_AND_OPEN_DECISIONS.md": "72_RISK_REGISTER_AND_OPEN_DECISIONS.md",
    "54_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md": "73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md",
    "55_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md": "74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md",
    "16_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md": "80_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md",
    "26_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md": "81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md",
    "41_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md": "82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md",
    "28_DEFINITION_OF_DONE_AND_ACCEPTANCE.md": "83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md",
    "44_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md": "84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md",
    "45_AGENT_TASK_EXECUTION_PROTOCOL.md": "85_AGENT_TASK_EXECUTION_PROTOCOL.md",
    "46_TASK_CARD_TEMPLATE.md": "86_TASK_CARD_TEMPLATE.md",
    "47_HANDOFF_AND_MANIFEST_PROTOCOL.md": "87_HANDOFF_AND_MANIFEST_PROTOCOL.md",
    "48_PARALLEL_AGENT_COORDINATION_RULES.md": "88_PARALLEL_AGENT_COORDINATION_RULES.md",
    "29_BUILD_AGENT_CONTEXT_LOADING_POLICY.md": "89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md",
    "52_PR_CHANGE_EVIDENCE_TEMPLATE.md": "90_PR_CHANGE_EVIDENCE_TEMPLATE.md",
    "51_FEATURE_COMPLETION_AUDIT.md": "91_FEATURE_COMPLETION_AUDIT.md",
    "25_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md": "92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md",
    "(new in V3.1)": "93_STATUS_VOCABULARY_AND_LIFECYCLE.md",
    "53_BUILD_MANIFEST.md": "98_BUILD_MANIFEST.md",
    "99_MANIFEST.md": "MANIFEST.md + manifest.json at repository root (covers all files, not only Part 2)",
}

def package_files():
    """Non-doc payload, excluding recursive manifest hashes and transient caches."""
    from pathlib import Path
    root = Path(ROOT)
    paths = [p for p in root.glob("*.md") if p.name != "MANIFEST.md"]
    paths += [root / "graph/project-graph.json", root / "graph/PROJECT_GRAPH.md"]
    paths += list((root / "tools").glob("*.py"))
    if (root / "evidence").exists():
        paths += [p for p in (root / "evidence").rglob("*") if p.is_file() and "__pycache__" not in p.parts]
    return sorted({str(p.relative_to(root)) for p in paths})


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def section_for(number):
    for lo, hi, key, title in SECTIONS:
        if lo <= number <= hi:
            return key, title
    return "unknown", "Unknown"


def title_of(path):
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("# "):
                return line[2:].strip()
    return os.path.basename(path)


def build():
    docs = sorted(f for f in os.listdir(DOCS) if f.endswith(".md"))
    entries = []
    for f in docs:
        m = re.match(r"^(\d{2})_", f)
        if not m:
            sys.exit("doc without numeric prefix: %s" % f)
        n = int(m.group(1))
        key, stitle = section_for(n)
        p = os.path.join(DOCS, f)
        entries.append({
            "path": "docs/" + f,
            "number": n,
            "section": key,
            "section_title": stitle,
            "title": title_of(p),
            "bytes": os.path.getsize(p),
            "sha256": sha256(p),
        })
    root_entries = []
    for rel in package_files():
        p = os.path.join(ROOT, rel)
        if not os.path.exists(p):
            sys.exit("missing package file: " + rel)
        root_entries.append({"path": rel, "bytes": os.path.getsize(p), "sha256": sha256(p)})

    # numbering collisions are a hard error
    seen = {}
    for e in entries:
        if e["number"] in seen:
            sys.exit("duplicate number %02d: %s and %s" % (e["number"], seen[e["number"]], e["path"]))
        seen[e["number"]] = e["path"]

    manifest = {
        "product": "Modbit",
        "edition": EDITION,
        "authority_date": AUTHORITY_DATE,
        "generated_on": date.today().isoformat(),
        "hash_algorithm": "sha256",
        "scope_rule": "docs/*.md, root *.md except MANIFEST.md, graph JSON/Markdown, tools/*.py, evidence payloads; manifests exclude themselves and transient caches",
        "counts": {
            "docs": len(entries),
            "root_and_tooling": len(root_entries),
            "docs_bytes": sum(e["bytes"] for e in entries),
        },
        "sections": [{"range": "%02d-%02d" % (lo, hi), "key": key, "title": t} for lo, hi, key, t in SECTIONS],
        "docs": entries,
        "root_and_tooling": root_entries,
        "renames_v3_to_v3_1": RENAMES,
    }
    return manifest


def render(manifest):
    entries = manifest["docs"]
    root_entries = manifest["root_and_tooling"]
    lines = []
    lines.append("# Modbit Dossier Manifest — %s" % manifest["edition"])
    lines.append("")
    lines.append("> **Authority date:** %s  " % manifest["authority_date"])
    lines.append("> **Generated:** %s by `tools/build_manifest.py`  " % manifest["generated_on"])
    lines.append("> **Scope:** every specification file in `docs/` plus the root governing files and tooling. "
                 "The previous `99_MANIFEST.md` covered only 39 Part 2 files; this manifest covers all %d docs." % len(entries))
    lines.append("> **Machine-readable twin:** `manifest.json` (same content, same hashes).")
    lines.append("")
    lines.append("## Integrity rule")
    lines.append("")
    lines.append("A dossier package is valid only if every path below exists with the listed SHA-256. "
                 "`python3 tools/check_dossier.py --manifest` verifies this. Regenerate after any edit with "
                 "`python3 tools/build_manifest.py`.")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append("| Section | Range | Files | Bytes |")
    lines.append("|---|---|---:|---:|")
    for lo, hi, key, t in SECTIONS:
        es = [e for e in entries if e["section"] == key]
        lines.append("| %s | %02d–%02d | %d | %d |" % (t, lo, hi, len(es), sum(e["bytes"] for e in es)))
    lines.append("| **Total docs** | | **%d** | **%d** |" % (len(entries), sum(e["bytes"] for e in entries)))
    lines.append("")
    lines.append("## Specification files (`docs/`)")
    lines.append("")
    lines.append("| # | File | Title | Section | Bytes | SHA-256 |")
    lines.append("|---:|---|---|---|---:|---|")
    for e in entries:
        lines.append("| %02d | `%s` | %s | %s | %d | `%s` |" % (
            e["number"], e["path"], e["title"].replace("|", "\\|"), e["section"], e["bytes"], e["sha256"]))
    lines.append("")
    lines.append("## Root governing files and tooling")
    lines.append("")
    lines.append("| File | Role | Bytes | SHA-256 |")
    lines.append("|---|---|---:|---|")
    roles = {
        "README.md": "human orientation",
        "AGENTS.md": "build-agent operating contract (highest authority)",
        "SKILLS.md": "governed procedures for agents",
        "graph/project-graph.json": "project driver graph with live status",
        "graph/PROJECT_GRAPH.md": "human view of the graph",
        "tools/build_manifest.py": "regenerates this manifest",
        "tools/build_graph.py": "regenerates graph structure from docs",
        "tools/graph.py": "query/update graph",
        "tools/check_dossier.py": "integrity gate",
        "tools/dossier_epr.py": "parses additive EPR authority and traceability",
        "tools/test_dossier.py": "copied-package integration and negative tests",
    }
    for e in root_entries:
        role = roles.get(e["path"], "retained evidence" if e["path"].startswith("evidence/") else "source patch provenance")
        lines.append("| `%s` | %s | %d | `%s` |" % (e["path"], role, e["bytes"], e["sha256"]))
    lines.append("")
    lines.append("## Rename map (V3 flat numbering → V3.1 `docs/`)")
    lines.append("")
    lines.append("V3 reused numbers 17–29 for two different file sets. V3.1 assigns one number per file, grouped by section. "
                 "Content was not changed by the move except cross-reference rewrites, removal of de-branding artifacts, "
                 "and the merge noted below.")
    lines.append("")
    lines.append("| V3 file | V3.1 location |")
    lines.append("|---|---|")
    for old in RENAMES:
        lines.append("| `%s` | `%s` |" % (old, RENAMES[old]))
    lines.append("")
    return "\n".join(lines)


def main():
    manifest = build()
    with open(os.path.join(ROOT, "manifest.json"), "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=2, ensure_ascii=False)
        fh.write("\n")
    with open(os.path.join(ROOT, "MANIFEST.md"), "w", encoding="utf-8") as fh:
        fh.write(render(manifest))
    print("MANIFEST.md + manifest.json written: %d docs, %d root/tooling files" % (
        len(manifest["docs"]), len(manifest["root_and_tooling"])))


if __name__ == "__main__":
    main()
