#!/usr/bin/env python3
"""Exercise the real dossier CLIs against disposable packages; no product proof.

Run: python3 tools/test_dossier.py
Uses only Python 3.9+ and the standard library. Negative cases never modify the
working dossier or weaken a production security check.
"""
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TASKS = "docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md"
QUALS = "docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md"


class DossierTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="modbit-dossier-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "package"
        # `.claude` holds agent sessions' own worktrees (whole checkouts,
        # changing under the copy): never part of the package.
        shutil.copytree(ROOT, self.root, ignore=shutil.ignore_patterns(
            ".git", "__pycache__", ".DS_Store", "node_modules", "target", ".claude"))

    def run_tool(self, tool, *args, ok=True, contains=None):
        result = subprocess.run([sys.executable, "tools/" + tool + ".py", *args],
                                cwd=self.root, capture_output=True, text=True, timeout=30)
        output = result.stdout + result.stderr
        if ok:
            self.assertEqual(result.returncode, 0, output)
        else:
            self.assertNotEqual(result.returncode, 0, output)
        if contains:
            self.assertIn(contains, output)
        return output

    def rebuild(self):
        self.run_tool("build_manifest")
        self.run_tool("build_graph")
        self.run_tool("build_manifest")

    def edit(self, rel, old, new):
        p = self.root / rel
        text = p.read_text()
        self.assertIn(old, text)
        p.write_text(text.replace(old, new, 1))

    def graph(self):
        return json.loads((self.root / "graph/project-graph.json").read_text())

    def write_graph(self, graph):
        (self.root / "graph/project-graph.json").write_text(json.dumps(graph))

    def test_real_generation_integrity_and_epr_neighborhood(self):
        self.rebuild()
        self.run_tool("check_dossier", "--manifest", contains="manifest hashes verified")
        out = self.run_tool("graph", "show", "EPR-006")
        for required in ("EPR-017", "EPR-009", "M3.9", "REQ-EPR-006", "QUAL-EPR-006",
                         "EPR-E2E-006", "EPR-FI-006", "core-runtime"):
            self.assertIn(required, out)
        g = self.graph()
        self.assertEqual(sum(n["type"] == "subsystem" for n in g["nodes"]), 23)
        self.assertEqual(sum(n["type"] == "release_gate" for n in g["nodes"]), 7)

    def test_regeneration_is_deterministic_and_preserves_live_state(self):
        self.run_tool("graph", "set", "DOC-EPR-002", "AUDITING", "--note", "fixture audit",
                      "--agent", "fixture-owner", "--evidence", "run:fixture-filesystem-proof")
        before = {n["id"]: n for n in self.graph()["nodes"]}
        self.rebuild()
        first = (self.root / "graph/project-graph.json").read_bytes()
        self.rebuild()
        self.assertEqual(first, (self.root / "graph/project-graph.json").read_bytes())
        after = {n["id"]: n for n in self.graph()["nodes"]}
        for nid, node in before.items():
            if "evidence" in node:
                for key in ("status", "evidence", "notes", "owner_agent", "status_changed_on"):
                    self.assertEqual(node.get(key), after[nid].get(key), (nid, key))

    def test_ev_ledger_and_original_patch_preserved(self):
        baseline = json.loads((self.root / "evidence/dossier-epr/baseline.json").read_text())["files"]
        for path in ("docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md",
                     "MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md"):
            self.assertEqual(hashlib.sha256((self.root / path).read_bytes()).hexdigest(), baseline[path])

    def test_missing_requirement_is_rejected(self):
        p = self.root / TASKS
        p.write_text("\n".join(line for line in p.read_text().splitlines()
                               if not line.startswith("| REQ-EPR-006 |")) + "\n")
        self.run_tool("check_dossier", ok=False, contains="D6")
        self.run_tool("build_graph", ok=False, contains="REQ-EPR-000..019")

    def test_duplicate_task_card_is_rejected(self):
        p = self.root / TASKS
        p.write_text(p.read_text() + "\n## EPR-006 — Duplicate task\n")
        self.run_tool("check_dossier", ok=False, contains="task cards")

    def test_wrong_qualification_requirement_is_rejected(self):
        self.edit(QUALS, "| QUAL-EPR-006 | REQ-EPR-006 |", "| QUAL-EPR-006 | REQ-EPR-007 |")
        self.run_tool("check_dossier", ok=False, contains="qualification owner/requirement mismatch")

    def test_owner_disagreement_is_rejected(self):
        self.edit(QUALS, "| QUAL-EPR-006 | REQ-EPR-006 | core-runtime |",
                  "| QUAL-EPR-006 | REQ-EPR-006 | model-gateway |")
        self.run_tool("build_graph", ok=False, contains="owner/requirement mismatch")

    def test_explicit_epr_cycle_is_rejected(self):
        self.edit(TASKS, "| M2 | 0 | M2.9 |", "| M2 | 0 | EPR-005 |")
        self.run_tool("build_graph", ok=False, contains="dependency cycle")

    def test_implicit_milestone_cycle_is_rejected(self):
        # EPR-004 is scheduled in M3 by DR-M2-002; M10 is the milestone that
        # depends on M3, so a prerequisite in M10 cycles.
        self.edit(TASKS, "| M2 | 1 | EPR-014,EPR-016 |", "| M2 | 1 | M10.1 |")
        self.run_tool("build_graph", ok=False, contains="dependency cycle")

    def test_gate_missing_task_is_rejected(self):
        self.edit(QUALS, "| EPR-001,EPR-002,EPR-004,EPR-005,EPR-008,EPR-014,EPR-017 |",
                  "| EPR-001,EPR-999 |")
        self.run_tool("build_graph", ok=False, contains="gate prerequisites")

    def test_missing_adr_is_rejected(self):
        p = self.root / "docs/02_AUTHORITY_AND_DECISIONS.md"
        p.write_text("\n".join(l for l in p.read_text().splitlines() if not l.startswith("| ADR-R-043 |")) + "\n")
        self.run_tool("build_graph", ok=False, contains="ADR-R-039..056")

    def test_source_change_requires_graph_regeneration(self):
        self.edit(TASKS, '## EPR-006 — Activate prevalidated escalation continuations', '## EPR-006 — Activate prevalidated escalation continuations (fixture change)')
        self.run_tool("check_dossier", ok=False, contains="graph structure differs")

    def test_manifest_detects_source_tampering(self):
        self.rebuild()
        p = self.root / "MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md"
        p.write_text(p.read_text() + "\nTampered fixture.\n")
        self.run_tool("check_dossier", "--manifest", ok=False, contains="hash mismatch")

    def test_manifest_detects_unlisted_payload(self):
        self.rebuild()
        (self.root / "evidence/unlisted.json").write_text("{}\n")
        self.run_tool("check_dossier", "--manifest", ok=False, contains="coverage mismatch")

    def test_human_manifest_cannot_drift_from_json(self):
        self.rebuild()
        p = self.root / "MANIFEST.md"
        p.write_text(p.read_text() + "\nStale human view.\n")
        self.run_tool("check_dossier", "--manifest", ok=False, contains="machine-readable twin")

    def test_graph_rejects_duplicate_and_dangling_nodes(self):
        g = self.graph()
        g["nodes"].append(g["nodes"][0])
        g["edges"].append({"from": "EPR-006", "to": "missing", "type": "after"})
        self.write_graph(g)
        out = self.run_tool("check_dossier", ok=False)
        self.assertIn("duplicate graph node", out)
        self.assertIn("dangling edge", out)

    def test_completion_and_upstream_guards_do_not_mutate_graph(self):
        before = (self.root / "graph/project-graph.json").read_bytes()
        # Any task still NOT_STARTED (an EPR task while one remains): which
        # one moves as the product is built, the guards do not.
        pending = [n["id"] for n in self.graph()["nodes"]
                   if n["type"] == "imp_task" and n.get("status") == "NOT_STARTED"]
        tid = next((i for i in pending if i.startswith("EPR-")), pending[0])
        self.run_tool("graph", "set", tid, "COMPLETE", ok=False, contains="requires at least one")
        # Whichever guard fires first (lifecycle step, upstream milestone or
        # prerequisite), the set is refused.
        self.run_tool("graph", "set", tid, "COMPLETE", "--evidence", "run:fixture-test",
                      ok=False, contains=f"cannot COMPLETE {tid}")
        self.assertEqual(before, (self.root / "graph/project-graph.json").read_bytes())

    def test_cross_task_readiness_and_status_guard(self):
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        outgoing = {}
        for edge in g["edges"]:
            outgoing.setdefault((edge["from"], edge["type"]), []).append(edge["to"])

        def satisfy(nid):
            node = nodes[nid]
            if node["type"] == "milestone":
                for edge in g["edges"]:
                    if edge["to"] == nid and edge["type"] in ("scheduled_in", "part_of"):
                        satisfy(edge["from"])
                return
            for mid in outgoing.get((nid, "scheduled_in"), []) + outgoing.get((nid, "part_of"), []):
                for upstream in outgoing.get((mid, "depends_on"), []):
                    satisfy(upstream)
            for dep in outgoing.get((nid, "after"), []):
                satisfy(dep)
            node["status"] = "COMPLETE"
            node["evidence"] = ["run:fixture-prerequisite-state-only"]

        for dep in outgoing[("EPR-005", "after")]:
            satisfy(dep)
        nodes["EPR-005"]["status"] = "NOT_STARTED"
        self.write_graph(g)
        self.assertIn("EPR-005", self.run_tool("graph", "ready"))
        nodes["EPR-004"]["status"] = "NOT_STARTED"
        self.write_graph(g)
        self.assertNotIn("EPR-005", self.run_tool("graph", "ready"))
        self.run_tool("graph", "set", "EPR-005", "AUDITING", ok=False, contains="prerequisite EPR-004")

    def test_dossier_completion_is_excluded_from_product_rollup(self):
        before = self.run_tool("graph", "status")
        self.run_tool("graph", "set", "DOC-EPR-002", "COMPLETE", "--evidence", "run:fixture-package-test")
        self.assertEqual(before, self.run_tool("graph", "status"))
        self.run_tool("check_dossier", contains="OK:")
        self.assertIn("DOC-EPR-002`: COMPLETE", (self.root / "graph/PROJECT_GRAPH.md").read_text())

    def test_v11_source_and_historical_evidence_preserved(self):
        baseline = json.loads((self.root / "evidence/dossier-epr-v1.1/baseline.json").read_text())["files"]
        paths = ["MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md",
                 "docs/05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md",
                 "docs/94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md"]
        paths += [p for p in baseline if p.startswith("evidence/dossier-epr/")]
        for path in paths:
            self.assertEqual(hashlib.sha256((self.root / path).read_bytes()).hexdigest(), baseline[path])

    def test_v11_provenance_and_delta_trace_are_generated(self):
        self.rebuild()
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        self.assertEqual(len([e for e in g["edges"] if e["type"] == "supersedes"]), 10)
        self.assertEqual(len([n for n in g["nodes"] if n["type"] == "source_patch"]), 2)
        self.assertEqual(nodes["MODBIT-PATCH-EPR-2026-09-05-v1.1"]["path"], "MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md")
        for i in range(14, 20):
            out = self.run_tool("graph", "show", "EPR-%03d" % i)
            for prefix in ("REQ-EPR-", "QUAL-EPR-", "EPR-E2E-", "EPR-FI-"):
                self.assertIn(prefix + "%03d" % i, out)
        self.assertEqual(nodes["DOC-EPR-001"]["status"], "COMPLETE")
        self.assertTrue(nodes["DOC-EPR-001"]["evidence"])

    def test_missing_scoped_supersession_is_rejected(self):
        p = self.root / "docs/02_AUTHORITY_AND_DECISIONS.md"
        p.write_text("\n".join(l for l in p.read_text().splitlines() if not l.startswith("| `ADR-R-053` |")) + "\n")
        self.run_tool("build_graph", ok=False, contains="scoped supersession links")

    def test_missing_statistics_replay_version_is_rejected(self):
        self.edit("docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md",
                  "statistics/compiler/gate/realized-risk versions", "compiler/gate/realized-risk versions")
        self.run_tool("check_dossier", ok=False, contains="contract missing required fields: ConditionalExecutionPlan")

    def test_legacy_runtime_algorithm_is_rejected(self):
        p = self.root / "docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md"
        p.write_text(p.read_text() + "\n## ExecuteCascade\n\nA superseded runtime fixture.\n")
        self.run_tool("build_graph", ok=False, contains="conditional algorithm surface")

    def test_independent_gate_calibration_cannot_be_removed(self):
        self.edit(QUALS, "| EPR-007,EPR-018,EPR-019 |", "| EPR-007,EPR-018 |")
        self.run_tool("build_graph", ok=False, contains="critical gate coverage missing")

    def test_resealing_cannot_bless_modified_v11_source(self):
        p = self.root / "MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md"
        p.write_text(p.read_text() + "\nAltered provenance fixture.\n")
        self.run_tool("build_manifest")
        self.run_tool("build_graph", ok=False, contains="immutable source hash mismatch")
        self.run_tool("check_dossier", "--manifest", ok=False, contains="immutable source hash mismatch")

    def test_malformed_evidence_reference_is_rejected(self):
        before = (self.root / "graph/project-graph.json").read_bytes()
        self.run_tool("graph", "set", "DOC-EPR-002", "AUDITING", "--evidence", "done",
                      ok=False, contains="must be <kind>:<value>")
        self.run_tool("graph", "set", "DOC-EPR-002", "AUDITING", "--evidence", "artifact:evidence/missing/proof.json",
                      ok=False, contains="does not exist")
        self.assertEqual(before, (self.root / "graph/project-graph.json").read_bytes())
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        nodes["DOC-EPR-001"]["evidence"].append("done")
        self.write_graph(g)
        self.run_tool("check_dossier", ok=False, contains="[G3] DOC-EPR-001")

    def test_non_vocabulary_decision_status_is_rejected(self):
        self.edit("docs/02_AUTHORITY_AND_DECISIONS.md", "| **EXPERIMENT** | No second general memory",
                  "| **PROVISIONAL / EXPERIMENT-GATED** | No second general memory")
        self.run_tool("build_graph", ok=False, contains="decision status outside")
        self.run_tool("check_dossier", ok=False, contains="[D7] MOD-SKILL-001")

    def test_governance_reseal_task_and_evidence_grammar(self):
        self.rebuild()
        self.run_tool("check_dossier", "--manifest", contains="manifest hashes verified")
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        self.assertEqual(nodes["DOC-EPR-002"]["status"], "COMPLETE")
        self.assertEqual(nodes["MOD-SKILL-001"]["status"], "EXPERIMENT")
        self.assertEqual(nodes["DOC-GOV-001"]["status"], "COMPLETE")
        self.assertIn("DOC-GOV-001", self.run_tool("graph", "show", "DOC-GOV-002"))
        self.assertIn("97_DOSSIER_MAINTENANCE_LOG.md", self.run_tool("graph", "show", "DR-GOV-2026-09-05-002"))
        self.assertEqual(nodes["DOC-GOV-002"]["status"], "COMPLETE")
        self.assertIn("DOC-GOV-002", self.run_tool("graph", "show", "DOC-GOV-003"))
        self.assertEqual(nodes["DOC-GOV-003"]["status"], "COMPLETE")
        self.assertIn("DOC-GOV-003", self.run_tool("graph", "show", "DOC-GOV-004"))
        self.assertEqual(nodes["DOC-GOV-004"]["status"], "COMPLETE")
        self.assertIn("DOC-GOV-004", self.run_tool("graph", "show", "DOC-PX-001"))
        self.assertIn("DOC-PX-001", self.run_tool("graph", "show", "DOC-PX-002"))
        self.assertIn("DOC-PX-002", self.run_tool("graph", "show", "DOC-PX-003"))
        self.assertIn("DOC-PX-003", self.run_tool("graph", "show", "DOC-PX-004"))
        self.assertIn("DOC-PX-004", self.run_tool("graph", "show", "DOC-PX-005"))
        self.assertIn("DOC-PX-005", self.run_tool("graph", "show", "DOC-PX-006"))
        out = self.run_tool("graph", "show", "DOC-GOV-005")
        for required in ("DOC-PX-006", "DR-GOV-2026-09-19-005", "77_RELEASE_ZERO_EXECUTION_GOAL.md", "97_DOSSIER_MAINTENANCE_LOG.md", "governance"):
            self.assertIn(required, out)
        out = self.run_tool("graph", "show", "DOC-GOV-006")
        for required in ("DOC-GOV-005", "DR-GOV-2026-09-24-006", "77_RELEASE_ZERO_EXECUTION_GOAL.md", "97_DOSSIER_MAINTENANCE_LOG.md", "governance"):
            self.assertIn(required, out)
        out = self.run_tool("graph", "show", "DOC-GOV-007")
        for required in ("DOC-GOV-006", "DR-GOV-2026-09-25-007", "77_RELEASE_ZERO_EXECUTION_GOAL.md", "97_DOSSIER_MAINTENANCE_LOG.md", "governance"):
            self.assertIn(required, out)
        out = self.run_tool("graph", "show", "DOC-GOV-001")
        for required in ("DOC-EPR-002", "DR-GOV-2026-09-05", "96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md", "governance"):
            self.assertIn(required, out)
        self.assertIn("DR-GOV-2026-09-05", self.run_tool("graph", "show", "MOD-SKILL-001"))
        for node in g["nodes"]:
            for ref in node.get("evidence") or []:
                self.assertRegex(ref, r"^(run|test|commit|revision|build|env|artifact|event|effect|checkpoint):\S+$", node["id"])

    def test_implementation_specs_carry_v11_placement(self):
        required = {
            "docs/12_REPOSITORY_AND_MODULE_LAYOUT.md": ("ConditionalExecutionPlan", "Outcome Statistics Store", "review_isolated", "EPR-018"),
            "docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md": ("Isolated Non-Committing Reviewer", "review_isolated"),
            "docs/21_TERMINAL_EXECUTION_AND_SANDBOX.md": ("`review_isolated`", "ADR-R-053"),
            "docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md": ("AcceptanceGateResult", "RealizedRisk", "stats_version"),
        }
        for rel, needles in required.items():
            body = (self.root / rel).read_text()
            for needle in needles:
                self.assertIn(needle, body, rel)
        for rel in ("docs/12_REPOSITORY_AND_MODULE_LAYOUT.md", "docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md",
                    "docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md"):
            body = (self.root / rel).read_text()
            self.assertNotIn("prior/realized risk", body, rel)
            self.assertNotIn("deterministic execution plan compiler", body, rel)

    def reset_m01_fixture(self):
        # Product work has started in the live graph. Tests that exercise the
        # lifecycle from a clean start reset every product work item in the copied
        # package to NOT_STARTED with a matching docs/98 roll-up; the delivered
        # package is never modified.
        g = self.graph()
        for node in g["nodes"]:
            if node["type"] in ("milestone_task", "imp_task"):
                node["status"] = "NOT_STARTED"
                node["evidence"] = []
                for key in ("notes", "blocked_from", "status_changed_on"):
                    node.pop(key, None)
        self.write_graph(g)
        bm = self.root / "docs/98_BUILD_MANIFEST.md"
        bm.write_text(re.sub(r"^(\| M\d+ \| .+? \| )[A-Z_]+( \|)", r"\g<1>NOT_STARTED\g<2>", bm.read_text(), flags=re.M))

    def test_lifecycle_transitions_are_one_step(self):
        self.reset_m01_fixture()
        self.run_tool("graph", "set", "M0.1", "IMPLEMENTING", ok=False, contains="one step at a time")
        self.run_tool("graph", "set", "M0.1", "AUDITING")
        self.run_tool("graph", "set", "M0.1", "BLOCKED", ok=False, contains="requires --note")
        self.run_tool("graph", "set", "M0.1", "BLOCKED", "--note", "fixture blocker with reproduction")
        self.run_tool("graph", "set", "M0.1", "IMPLEMENTING", ok=False, contains="resume at or below")
        self.run_tool("graph", "set", "M0.1", "AUDITING")
        self.run_tool("graph", "set", "M0.1", "NOT_STARTED", ok=False, contains="requires --note")
        self.run_tool("graph", "set", "M0.1", "NOT_STARTED", "--note", "fixture regression")
        self.run_tool("check_dossier", contains="OK:")

    def test_gate_attestation_and_gated_rollup(self):
        before = (self.root / "graph/project-graph.json").read_bytes()
        # A gate whose required tasks are still open (G waits on EPR-012 and
        # EPR-013 in M10); A's tasks sealed with M4 and E's with M9, so neither
        # can stand in for "not COMPLETE" any more.
        self.run_tool("graph", "attest", "EPR-GATE-G", "--evidence", "run:fixture-gate", ok=False, contains="required tasks not COMPLETE")
        self.run_tool("graph", "attest", "EPR-GATE-G", ok=False, contains="at least one --evidence")
        self.run_tool("graph", "attest", "EPR-006", "--evidence", "run:fixture-gate", ok=False, contains="release gates only")
        self.assertEqual(before, (self.root / "graph/project-graph.json").read_bytes())
        out = self.run_tool("graph", "gates")
        self.assertIn("EPR-GATE-G", out)
        self.assertIn("OPEN", out)
        self.assertIn("M10 is gated by", out)
        g = self.graph()
        for node in g["nodes"]:
            if node["type"] in ("imp_task", "milestone_task"):
                node["status"] = "COMPLETE"
                node["evidence"] = ["run:fixture-all-complete"]
        self.write_graph(g)
        status = self.run_tool("graph", "status")
        self.assertRegex(status, r"M10\s+GATED")
        self.assertRegex(status, r"M9\s+COMPLETE")
        for letter in "ABCDEFG":
            self.run_tool("graph", "attest", "EPR-GATE-" + letter, "--evidence",
                          "artifact:evidence/dossier-gov/validation.json", "--agent", "fixture-release-agent")
        self.assertRegex(self.run_tool("graph", "status"), r"M10\s+COMPLETE")
        self.assertIn("SATISFIED", self.run_tool("graph", "gates"))
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        nodes["EPR-019"]["status"] = "NOT_STARTED"
        nodes["EPR-019"]["evidence"] = []
        self.write_graph(g)
        self.run_tool("check_dossier", ok=False, contains="[G7] EPR-GATE-D")

    def test_patch_coverage_map_is_complete(self):
        self.run_tool("check_dossier", contains="OK:")
        p = self.root / "docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md"
        p.write_text("\n".join(l for l in p.read_text().splitlines() if not l.startswith("| v1.1 §22 |")) + "\n")
        self.run_tool("check_dossier", ok=False, contains="[D8] v1.1 patch section 22")
        for rel in ("docs/10_PRODUCT_PRD_AND_UX.md", "docs/24_CLOUD_CONTROL_PLANE_AND_SYNC.md", "docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md"):
            self.assertIn("quality floor", (self.root / rel).read_text(), rel)

    def test_px_ledger_structure_is_enforced(self):
        ledger = "docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md"
        p = self.root / ledger
        original = p.read_text()
        p.write_text(original.replace("## PX-000 — ", "## PX-000-renamed — ", 1))
        self.run_tool("build_graph", ok=False, contains="task card")
        self.run_tool("check_dossier", ok=False, contains="[D9]")
        p.write_text(original.replace("| M2 | ALPHA | ADOPT |", "| M2 | ALPHA | DEFERRED |", 1))
        self.run_tool("build_graph", ok=False, contains="DEFERRED row")
        p.write_text(original.replace("| REQ-PX-000 |", "| REQ-PX-999 |", 1).replace("| QUAL-PX-000 | REQ-PX-000 |", "| QUAL-PX-999 | REQ-PX-999 |", 1))
        self.run_tool("build_graph", ok=False, contains="contiguous")

    def test_adopt_row_without_owner_is_rejected(self):
        ledger = "docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md"
        self.edit(ledger, "| REQ-EV-0208 | No second general memory system | ADOPT | Architecture Governance |",
                  "| REQ-EV-0208 | No second general memory system | ADOPT |  |")
        self.run_tool("check_dossier", ok=False, contains="[D10]")
        self.run_tool("build_graph", ok=False, contains="OWNER_MAP is missing")

    def test_complete_without_qualifying_evidence_is_rejected(self):
        g = self.graph()
        for node in g["nodes"]:
            if node["id"] == "IMP-EV-0208":
                node["status"] = "COMPLETE"
                node["evidence"] = ["artifact:README.md"]
        self.write_graph(g)
        out = self.run_tool("check_dossier", ok=False, contains="[G9]")
        self.assertIn("without a run:/test: evidence ref", out)
        self.assertIn("without a commit:/revision: evidence ref", out)
        for node in g["nodes"]:
            if node["id"] == "IMP-EV-0208":
                node["evidence"] = ["run:fixture-run", "commit:abc123"]
        self.write_graph(g)
        result = subprocess.run([sys.executable, "tools/check_dossier.py"], cwd=self.root, capture_output=True, text=True, timeout=30)
        self.assertNotIn("[G9]", result.stdout + result.stderr)

    def test_release_readiness_is_derived_from_tasks(self):
        self.reset_m01_fixture()
        out = self.run_tool("graph", "releases")
        self.assertRegex(out, r"ALPHA\s+NOT_READY")
        self.assertRegex(out, r"RELEASE_ZERO\s+NOT_READY")
        scoped = self.run_tool("graph", "ready", "--release", "ALPHA")
        self.assertIn("M0.1", scoped)
        self.assertNotIn("DOC-PX-001", scoped)
        self.run_tool("graph", "ready", "--release", "NOPE", ok=False, contains="unknown release")
        self.run_tool("graph", "set", "ALPHA", "COMPLETE", "--evidence", "run:fixture", ok=False, contains="status is only tracked on work items")
        g = self.graph()
        alpha = {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"}
        self.assertIn("M1.1", alpha)
        self.assertIn("PX-000", alpha)
        self.assertNotIn("M2.10", alpha)
        self.assertFalse(any(i.startswith("EPR-") for i in alpha))
        nodes = {n["id"]: n for n in g["nodes"]}
        for i in alpha:
            nodes[i]["status"] = "COMPLETE"
            nodes[i]["evidence"] = ["run:fixture-alpha"]
        self.write_graph(g)
        out = self.run_tool("graph", "releases")
        self.assertRegex(out, r"ALPHA\s+READY")
        self.assertRegex(out, r"RELEASE_ZERO\s+NOT_READY")
        nodes["M1.1"]["status"] = "BLOCKED"
        self.write_graph(g)
        self.assertRegex(self.run_tool("graph", "releases"), r"ALPHA\s+BLOCKED")
        for n in g["nodes"]:
            if n["type"] in ("imp_task", "milestone_task"):
                n["status"] = "COMPLETE"
                n["evidence"] = ["run:fixture-all"]
        self.write_graph(g)
        self.assertRegex(self.run_tool("graph", "releases"), r"RELEASE_ZERO\s+NOT_READY")
        for letter in "ABCDEFG":
            self.run_tool("graph", "attest", "EPR-GATE-" + letter, "--evidence", "artifact:evidence/dossier-gov/validation.json")
        self.assertRegex(self.run_tool("graph", "releases"), r"RELEASE_ZERO\s+READY")

    def test_work_item_outside_release_zero_is_rejected(self):
        g = self.graph()
        g["nodes"].append({"id": "IMP-EV-9999", "type": "imp_task", "title": "orphan fixture", "status": "NOT_STARTED", "evidence": []})
        g["edges"].append({"from": "IMP-EV-9999", "to": "core-runtime", "type": "owned_by"})
        g["edges"].append({"from": "IMP-EV-9999", "to": "M2", "type": "scheduled_in"})
        self.write_graph(g)
        self.run_tool("check_dossier", ok=False, contains="[G8] IMP-EV-9999")

    def test_deferred_px_rows_have_no_tasks_and_no_release(self):
        g = self.graph()
        nodes = {n["id"]: n for n in g["nodes"]}
        self.assertEqual(nodes["REQ-PX-003"]["disposition"], "DEFERRED")
        self.assertFalse([e for e in g["edges"] if e["from"] == "REQ-PX-003" and e["type"] == "implemented_by"])
        self.assertNotIn("PX-003", nodes)
        self.assertIn("QUAL-PX-003", nodes)
        included = {e["to"] for e in g["edges"] if e["type"] == "includes"}
        self.assertIn("PX-002", included)
        self.assertNotIn("PX-002", {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"})
        self.assertIn("PX-002", {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "BETA"})
        out = self.run_tool("graph", "show", "PX-007")
        for needle in ("PX-006", "M2.2", "QUAL-PX-007", "PX-E2E-007", "workspace-git", "DR-PX-2026-09-05"):
            self.assertIn(needle, out)

    def test_competence_contracts_are_specified_and_wired(self):
        out = self.run_tool("graph", "show", "PX-018")
        for needle in ("PX-017", "QUAL-PX-018", "PX-E2E-018", "core-runtime", "ALPHA" if False else "M2"):
            self.assertIn(needle, out)
        d28 = (self.root / "docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md").read_text()
        for needle in ("RepairAttempt", "failure_signature", "hypothesis", "Equivalent repeated hypotheses", "SelfReview"):
            self.assertIn(needle, d28)
        d63 = (self.root / "docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md").read_text()
        self.assertIn("Baseline-then-target", d63)
        self.assertIn("repair_attempts", (self.root / "docs/31_DATABASE_AND_STORAGE_SCHEMA.md").read_text())
        alpha = {e["to"] for e in self.graph()["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"}
        for tid in ("PX-014", "PX-016", "PX-017", "PX-018", "PX-019"):
            self.assertIn(tid, alpha)
        self.assertNotIn("PX-020", alpha)

    def test_ux_flows_are_specified_and_measurable(self):
        d39 = (self.root / "docs/39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md").read_text()
        for needle in ("five minutes", "degraded", "recovery", "Notification model", "Keyboard model", "Interaction budgets"):
            self.assertIn(needle, d39)
        self.assertIn("five minutes", (self.root / "docs/10_PRODUCT_PRD_AND_UX.md").read_text())
        out = self.run_tool("graph", "show", "PX-022")
        for needle in ("M1.4", "M2.9", "QUAL-PX-022", "PX-E2E-022", "desktop"):
            self.assertIn(needle, out)
        alpha = {e["to"] for e in self.graph()["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"}
        self.assertIn("PX-022", alpha)
        self.assertNotIn("PX-025", alpha)

    def test_language_and_platform_matrix_is_earned_not_assumed(self):
        d76 = (self.root / "docs/76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md").read_text()
        for needle in ("Tier A", "Tier B", "Tier C", "Unsupported", "CI_COMPATIBLE", "RELEASE_GRADE", "only after"):
            self.assertIn(needle, d76)
        g = self.graph()
        alpha = {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"}
        self.assertIn("PX-026", alpha)
        self.assertNotIn("PX-030", alpha)  # DR-M0-005: M10 / RELEASE_ZERO
        self.assertNotIn("PX-028", alpha)
        self.assertNotIn("PX-031", alpha)
        release_zero = {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "RELEASE_ZERO"}
        self.assertIn("PX-030", release_zero)
        out = self.run_tool("graph", "show", "PX-030")
        for needle in ("M0.1", "governance", "QUAL-PX-030", "M10", "RELEASE_ZERO"):
            self.assertIn(needle, out)

    def test_verification_execution_and_harness_contracts_are_wired(self):
        d64 = (self.root / "docs/64_VERIFICATION_EXECUTION_CONTRACTS.md").read_text()
        for needle in ("BASELINE", "COMPLETION", "TestReport", "failure_signature", "FLAKY", "DI-3", "REGRESSION", "KNOWN_FAILING"):
            self.assertIn(needle, d64)
        self.assertIn("Agent harness contracts", (self.root / "docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md").read_text())
        d28 = (self.root / "docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md").read_text()
        for needle in ("RepairPolicy", "ScopePolicy", "original plan", "NoProgressDetected", "Reproduction first"):
            self.assertIn(needle, d28)
        self.assertIn("Two baselines", (self.root / "docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md").read_text())
        g = self.graph()
        alpha = {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "ALPHA"}
        beta = {e["to"] for e in g["edges"] if e["type"] == "includes" and e["from"] == "BETA"}
        for tid in ("PX-032", "PX-033", "PX-034", "PX-036", "PX-037", "PX-038", "PX-039", "PX-040", "IMP-EV-0107"):
            self.assertIn(tid, alpha)
        self.assertNotIn("PX-035", alpha)
        self.assertIn("PX-035", beta)
        scheduled = {(e["from"], e["to"]) for e in g["edges"] if e["type"] == "scheduled_in"}
        self.assertIn(("IMP-EV-0107", "M2"), scheduled)
        nodes = {n["id"]: n for n in g["nodes"]}
        self.assertIn("DR-PX-2026-09-05-006", nodes["IMP-EV-0107"]["milestone_override"])
        self.assertEqual(nodes["IMP-EV-0242"]["milestone"], "M4")
        self.assertIn("DR-M0-005", nodes["IMP-EV-0242"]["milestone_override"])
        self.assertEqual(nodes["IMP-EV-0119"]["milestone"], "M2")
        self.assertEqual(nodes["IMP-EV-0180"]["milestone"], "M6")
        self.assertIn("DR-M1-006", nodes["IMP-EV-0180"]["milestone_override"])
        out = self.run_tool("graph", "show", "PX-033")
        for needle in ("IMP-EV-0107", "M2.8", "QUAL-PX-033", "PX-E2E-033", "verification", "DR-PX-2026-09-05-006"):
            self.assertIn(needle, out)
        self.assertIn("DR-PX-2026-09-05", self.run_tool("graph", "show", "DR-PX-2026-09-05-006"))
        # DR-M2-002 moved PX-033 to M3; IMP-EV-0107 stays scheduled ahead of it in M2 by
        # its own override, and the graph records both reasons.
        self.assertEqual(nodes["PX-033"]["milestone"], "M3")
        self.assertIn("DR-M2-002", nodes["PX-033"]["milestone_override"])
        self.assertIn(("PX-033", "M3"), scheduled)
        self.assertIn("PX-033", alpha)

    def test_goal_is_derived_executable_and_release_scoped(self):
        # DOC-GOV-005: `graph.py goal` derives the remaining ladder to a release from live
        # state and exits 0 only when the release is READY (docs/77). Reset to a clean start
        # so the expectations do not depend on live progress.
        self.reset_m01_fixture()
        self.run_tool("graph", "goal", "NOPE", ok=False, contains="unknown release")
        before = (self.root / "graph/project-graph.json").read_bytes()
        out = self.run_tool("graph", "goal", ok=False)
        self.assertIn("GOAL RELEASE_ZERO", out)
        self.assertRegex(out, r"state\s+NOT_READY \(exit 1\)")
        self.assertRegex(out, r"(?m)^\s+0 M0\.1\s+milestone_task\s+NOT_STARTED\s+M0", )
        self.assertRegex(out, r"(?m)^\s+\d+ M1\.1\s+milestone_task\s+NOT_STARTED\s+M1\s+milestone:M0", )
        self.assertRegex(out, r"(?m)^\s+\d+ EPR-GATE-G\s+release_gate\s+OPEN", )
        self.assertRegex(out, r"(?m)^\s+\d+ RELEASE_ZERO\s+release\s+NOT_READY", )
        self.assertIn("python3 tools/graph.py show M0.1", out)
        self.assertEqual(before, (self.root / "graph/project-graph.json").read_bytes())
        result = subprocess.run([sys.executable, "tools/graph.py", "goal", "--json"], cwd=self.root, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual((report["goal"], report["state"], report["exit_code"]), ("RELEASE_ZERO", "NOT_READY", 1))
        steps = {s["id"]: s for s in report["steps"]}
        self.assertEqual(steps["M0.1"]["wave"], 0)
        self.assertIn("M0.1", report["startable_now"])
        self.assertNotIn("M0.2", report["startable_now"])
        self.assertEqual(steps["M0.2"]["waiting_on"], ["M0.1"])
        self.assertGreater(steps["M10.6"]["wave"], steps["M10.5"]["wave"])
        self.assertIn("EPR-019", steps["EPR-GATE-D"]["waiting_on"])
        self.assertEqual(report["final_step"]["id"], "RELEASE_ZERO")
        # A BLOCKED step is a blocker; everything downstream is held by it and the goal exits 2.
        self.run_tool("graph", "set", "M0.1", "BLOCKED", "--note", "fixture blocker: hosted CI billing")
        out = self.run_tool("graph", "goal", ok=False)
        self.assertRegex(out, r"state\s+BLOCKED \(exit 2\)")
        self.assertIn("M0.1 (M0) BLOCKED since", out)
        self.assertIn("fixture blocker: hosted CI billing", out)
        self.assertRegex(out, r"M0\.2\s+milestone_task\s+NOT_STARTED\s+M0\s+M0\.1 .*\[held by M0\.1\]")
        self.assertRegex(out, r"M1\.1\s+milestone_task.*\[held by M0\.1\]")
        self.assertIn("resolve M0.1", out)
        # Scoped to one release: ALPHA has no gates and never waits on M10.
        out = self.run_tool("graph", "goal", "ALPHA", ok=False)
        self.assertIn("GOAL ALPHA", out)
        self.assertNotIn("M10.6", out)
        self.assertNotIn("EPR-GATE-G", out)
        # Every work item COMPLETE and every gate attested: the goal is met and exits 0.
        g = self.graph()
        for node in g["nodes"]:
            if node["type"] in ("imp_task", "milestone_task"):
                node["status"] = "COMPLETE"
                node["evidence"] = ["run:fixture-all", "commit:fixture"]
                node.pop("blocked_from", None)
        self.write_graph(g)
        out = self.run_tool("graph", "goal", ok=False, contains="EPR-GATE-A TASKS_COMPLETE")
        self.assertRegex(out, r"(?m)^\s+0 EPR-GATE-A\s+release_gate\s+TASKS_COMPLETE", )
        self.assertIn("python3 tools/graph.py attest EPR-GATE-A", out)
        for letter in "ABCDEFG":
            self.run_tool("graph", "attest", "EPR-GATE-" + letter, "--evidence", "artifact:evidence/dossier-gov-005/baseline.json")
        out = self.run_tool("graph", "goal", contains="RELEASE_ZERO is READY")
        self.assertRegex(out, r"state\s+READY \(exit 0\)")
        self.assertRegex(self.run_tool("graph", "goal", "ALPHA"), r"state\s+READY \(exit 0\)")


class ExampleRunnerTests(unittest.TestCase):
    """QUAL-EV-0212: the declared-example runner catches every way an example
    can become a placeholder. Each case builds a minimal package holding only
    the runner, `graph.py` (whose parser the shape and synopsis checks ask) and
    two throwaway tools, so a negative case is fast and names one cause."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="modbit-example-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "package"
        (self.root / "tools").mkdir(parents=True)
        for tool in ("example_runner.py", "graph.py"):
            shutil.copy2(ROOT / "tools" / tool, self.root / "tools" / tool)
        (self.root / "tools" / "ok.py").write_text(
            "import sys\nif '--gone' in sys.argv:\n    pass\nsys.exit(0)\n", encoding="utf-8")
        (self.root / "tools" / "bad.py").write_text("import sys\nsys.exit(3)\n", encoding="utf-8")

    def readme(self, *blocks):
        body = "# fixture\n\n"
        for b in blocks:
            body += "```bash\n" + b.strip("\n") + "\n```\n\n"
        (self.root / "README.md").write_text(body, encoding="utf-8")

    def declare(self, *commands):
        """Declare README block 0 with `commands` as (text, declaration) pairs,
        hashing the block exactly as the runner does."""
        text = (self.root / "README.md").read_text(encoding="utf-8")
        body = re.search(r"^```bash[ \t]*\n(.*?)^```[ \t]*$", text, re.M | re.S).group(1)
        (self.root / "tools" / "examples.json").write_text(json.dumps({"examples": [{
            "file": "README.md", "index": 0,
            "sha256": hashlib.sha256(body.encode("utf-8")).hexdigest(),
            "commands": [dict(command=c, **d) for c, d in commands],
        }]}, indent=1), encoding="utf-8")

    def run_runner(self, *args):
        # These cases have the runner as their subject, so the nesting guard it
        # sets for a `run` example does not apply to them.
        env = {k: v for k, v in os.environ.items() if k != "MODBIT_EXAMPLE_RUNNER"}
        out = subprocess.run([sys.executable, "tools/example_runner.py", *args],
                             cwd=self.root, capture_output=True, text=True, timeout=300,
                             env=env)
        return out.returncode, out.stdout + out.stderr

    def assertFails(self, contains):
        code, output = self.run_runner()
        self.assertEqual(code, 1, output)
        self.assertIn(contains, output)
        return output

    def test_a_command_hidden_in_another_block_kind_fails(self):
        (self.root / "README.md").write_text(
            "# fixture\n\n```text\npython3 tools/ok.py\n```\n", encoding="utf-8")
        (self.root / "tools" / "examples.json").write_text('{"examples": []}', encoding="utf-8")
        self.assertFails("where this gate cannot see it")

    def test_an_undeclared_example_fails(self):
        self.readme("python3 tools/ok.py")
        (self.root / "tools" / "examples.json").write_text('{"examples": []}', encoding="utf-8")
        self.assertFails("undeclared example")

    def test_a_declared_example_that_changed_fails_as_drift(self):
        self.readme("python3 tools/ok.py")
        self.declare(("python3 tools/ok.py", {"policy": "run", "reason": ""}))
        self.readme("python3 tools/ok.py --gone")   # the doc changed, the declaration did not
        self.assertFails("drifted from its declaration")

    def test_a_declaration_whose_block_is_gone_fails(self):
        self.readme("python3 tools/ok.py")
        self.declare(("python3 tools/ok.py", {"policy": "run", "reason": ""}))
        (self.root / "README.md").write_text("# fixture\n\nno examples here\n", encoding="utf-8")
        self.assertFails("declared but no longer in the file")

    def test_a_run_example_that_stopped_working_fails(self):
        self.readme("python3 tools/bad.py")
        self.declare(("python3 tools/bad.py", {"policy": "run", "reason": ""}))
        self.assertFails("exited 3, not the declared 0")

    def test_a_declared_non_zero_contract_passes_and_must_be_justified(self):
        self.readme("python3 tools/bad.py")
        self.declare(("python3 tools/bad.py", {"policy": "run", "exit": 3, "reason": ""}))
        self.assertFails("expects a non-zero exit without saying why")
        self.declare(("python3 tools/bad.py",
                      {"policy": "run", "exit": 3, "reason": "3 is what it documents"}))
        code, output = self.run_runner()
        self.assertEqual(code, 0, output)
        self.assertIn("1 command(s) executed", output)

    def test_a_shape_example_without_a_reason_fails(self):
        self.readme("python3 tools/graph.py set IMP-EV-0012 REAL_TESTING")
        self.declare(("python3 tools/graph.py set IMP-EV-0012 REAL_TESTING",
                      {"policy": "shape", "reason": "   "}))
        self.assertFails("without saying why it cannot be executed")

    def test_a_cargo_example_naming_an_unknown_package_fails(self):
        # The cargo checker asks only what this repository owns: a real member.
        (self.root / "Cargo.toml").write_text(
            'members = [\n    "crates/real",\n]\n', encoding="utf-8")
        (self.root / "crates" / "real").mkdir(parents=True)
        (self.root / "crates" / "real" / "Cargo.toml").write_text(
            'name = "modbit-real"\n', encoding="utf-8")
        self.readme("cargo run -p modbit-real")
        self.declare(("cargo run -p modbit-real", {"policy": "shape", "reason": "CI owns the build"}))
        code, output = self.run_runner()
        self.assertEqual(code, 0, output)
        self.readme("cargo run -p modbit-ghost")
        self.declare(("cargo run -p modbit-ghost", {"policy": "shape", "reason": "CI owns the build"}))
        self.assertFails("is not a workspace member")

    def test_a_program_with_no_shape_checker_cannot_be_declared_shape(self):
        self.readme("git push origin main")
        self.declare(("git push origin main",
                      {"policy": "shape", "reason": "pushing in a gate would be absurd"}))
        self.assertFails("has no shape checker")

    def test_a_flag_a_tool_no_longer_parses_fails(self):
        self.readme("python3 tools/ok.py --gone")
        self.declare(("python3 tools/ok.py --gone",
                      {"policy": "shape", "reason": "illustrative"}))
        code, output = self.run_runner()
        self.assertEqual(code, 0, output)          # the literal is still in ok.py
        (self.root / "tools" / "ok.py").write_text("import sys\nsys.exit(0)\n", encoding="utf-8")
        self.assertFails("does not parse --gone")

    def test_a_shape_example_the_tools_own_parser_rejects_fails(self):
        self.readme("python3 tools/graph.py set IMP-EV-0012")
        self.declare(("python3 tools/graph.py set IMP-EV-0012",
                      {"policy": "shape", "reason": "illustrative ids"}))
        self.assertFails("own parser rejects")

    def test_a_synopsis_naming_an_unknown_flag_fails(self):
        self.readme("python3 tools/graph.py set <id> <STATE> [--proof <ref> ...]")
        self.declare(("python3 tools/graph.py set <id> <STATE> [--proof <ref> ...]",
                      {"policy": "synopsis", "reason": "a usage template"}))
        self.assertFails("which `graph.py set` does not accept")

    def test_a_synopsis_naming_an_unknown_subcommand_fails(self):
        self.readme("python3 tools/graph.py promote <id> [--evidence <ref> ...]")
        self.declare(("python3 tools/graph.py promote <id> [--evidence <ref> ...]",
                      {"policy": "synopsis", "reason": "a usage template"}))
        self.assertFails("names no graph.py subcommand")

    def test_a_synopsis_carrying_a_real_argument_is_refused(self):
        self.readme("python3 tools/graph.py set IMP-EV-0012 [--evidence <ref> ...]")
        self.declare(("python3 tools/graph.py set IMP-EV-0012 [--evidence <ref> ...]",
                      {"policy": "synopsis", "reason": "a usage template"}))
        self.assertFails("which a synopsis should not carry")



class IntegrationGateTests(unittest.TestCase):
    """QUAL-EV-0211: the multi-level integration gate refuses every way an
    integration can be qualified mock-only. Each case builds a minimal
    package — one crate that opens a connection, its tests, a live workflow,
    a Decision Record, a recorded fixture and the retained record of the run
    that made it — then breaks one thing, so a failure names one cause."""

    TESTS = """\
#[test]
fn component_calls_the_adapter() {}

#[tokio::test]
async fn integration_through_the_core() {}

#[tokio::test]
async fn live_against_the_real_api() {}

#[test]
fn replay_the_recording() {}

fn helper() {}
"""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="modbit-integration-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "package"
        self.write("tools/integration_gate.py", (ROOT / "tools" / "integration_gate.py").read_text(encoding="utf-8"))
        self.write("Cargo.toml", '[workspace]\nmembers = [\n    "crates/net",\n]\n')
        self.write("crates/net/Cargo.toml", '[package]\nname = "net-crate"\n')
        self.write("crates/net/src/lib.rs", "pub fn go() { let _ = reqwest::Client::new(); }\n")
        self.write("crates/net/tests/t.rs", self.TESTS)
        self.write(".github/workflows/live.yml",
                   "jobs:\n  live:\n    steps:\n      - run: cargo test --locked -p net-crate live_ -- --nocapture\n")
        self.write("docs/decisions/DR-X-001-defer.md", "---\nid: DR-X-001\nstatus: accepted\n---\n# defer\n")
        self.fixture = {
            "schema": "modbit.recorded-exchange/1", "integration": "api.example",
            "provenance": {"run": "123456", "endpoint_host": "api.example.com", "model": "m"},
            "exchanges": [{"request": {"method": "POST", "path": "/v1/x",
                                       "headers": {"content-type": "application/json"}, "body": {"q": 1}},
                           "response": {"status": 200, "headers": {"content-type": "text/event-stream"},
                                        "chunks": [{"at_ms": 1, "text": "data: {}\n\n"}]}}],
        }
        self.entry = {
            "id": "api.example", "crates": ["net-crate"], "boundary": "external-api",
            "counterparty": "a real API", "credentials": ["EXAMPLE_KEY"],
            "levels": {"component": ["crates/net/tests/t.rs::component_calls_the_adapter"],
                       "integration": ["crates/net/tests/t.rs::integration_through_the_core"],
                       "external": ["crates/net/tests/t.rs::live_against_the_real_api"]},
            "recorded": [{"fixture": "crates/net/tests/fixtures/x.json",
                          "replay": "crates/net/tests/t.rs::replay_the_recording"}],
        }
        self.save()

    def write(self, path, text):
        f = self.root / path
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(text, encoding="utf-8")

    def save(self, retain=True):
        """Write the inventory and the fixture; retain the run's record of the
        fixture's digest unless told not to."""
        self.write("tools/integrations.json", json.dumps({"integrations": [self.entry]}, indent=1))
        raw = json.dumps(self.fixture, indent=1) + "\n"
        self.write("crates/net/tests/fixtures/x.json", raw)
        record = self.record("passed", sha=hashlib.sha256(raw.encode("utf-8")).hexdigest())
        if retain:
            self.write("evidence/run-123456/live_against_the_real_api.json", json.dumps(record))
        return record

    def record(self, outcome, sha=None, proven=("api.example",)):
        return {"schema": "modbit.live-result/1", "test": "live_against_the_real_api",
                "outcome": outcome, "run": "123456",
                "proven": [{"integration": i} for i in proven],
                "recordings": [{"integration": "api.example", "file": "x.json", "sha256": sha or "0" * 64}]}

    def run_gate(self, *args):
        out = subprocess.run([sys.executable, "tools/integration_gate.py", *args], cwd=self.root,
                             capture_output=True, text=True, timeout=120)
        return out.returncode, out.stdout + out.stderr

    def live_dir(self, record):
        d = self.root / "live"
        d.mkdir(exist_ok=True)
        if record is not None:
            (d / "live_against_the_real_api.json").write_text(json.dumps(record), encoding="utf-8")
        return str(d)

    def assertFails(self, contains, *args):
        code, output = self.run_gate(*args)
        self.assertEqual(code, 1, output)
        self.assertIn(contains, output)
        return output

    def test_a_fully_qualified_integration_passes_structurally_and_live(self):
        code, output = self.run_gate()
        self.assertEqual(code, 0, output)
        code, output = self.run_gate("--live", self.live_dir(self.record("passed")))
        self.assertEqual(code, 0, output)

    def test_code_that_opens_a_connection_undeclared_fails(self):
        self.write("tools/integrations.json", json.dumps({"integrations": []}))
        self.assertFails("undeclared integration: net-crate opens a connection (reqwest at crates/net/src/lib.rs:1)")

    def test_a_typescript_package_that_fetches_undeclared_fails(self):
        self.write("pnpm-workspace.yaml", 'packages:\n  - "packages/*"\n')
        self.write("packages/web/package.json", '{"name": "@x/web"}')
        self.write("packages/web/src/api.ts", "export const get = () => fetch('https://x.example');\n")
        self.assertFails("undeclared integration: @x/web opens a connection (fetch at packages/web/src/api.ts:1)")

    def test_a_claim_on_a_crate_without_a_connection_is_stale(self):
        self.write("crates/net/src/lib.rs", "pub fn go() {}\n")
        self.assertFails("claims net-crate, whose source opens no outbound connection")

    def test_a_missing_test_fails(self):
        self.entry["levels"]["component"] = ["crates/net/tests/t.rs::nope"]
        self.save()
        self.assertFails("component level: crates/net/tests/t.rs: no test `nope`")

    def test_a_helper_function_is_not_a_test(self):
        self.entry["levels"]["integration"] = ["crates/net/tests/t.rs::helper"]
        self.save()
        self.assertFails("`helper` is a function, not a test")

    def test_an_external_api_without_a_live_level_or_a_deferral_is_mock_only(self):
        self.entry["levels"]["external"] = []
        self.entry["recorded"] = []
        self.save()
        out = self.assertFails("no external-level test against the real API (mock-only cannot pass)")
        self.assertIn("no recorded safe fixture of the real API", out)

    def test_an_external_api_without_a_component_level_fails(self):
        self.entry["levels"]["component"] = []
        self.save()
        self.assertFails("api.example: no component-level test")

    def test_a_deferral_needs_an_accepted_decision_record(self):
        self.entry["levels"]["external"] = []
        self.entry["recorded"] = []
        self.entry["deferred"] = {"decision": "DR-X-001", "needs": "a token"}
        self.save()
        code, output = self.run_gate()
        self.assertEqual(code, 0, output)
        self.write("docs/decisions/DR-X-001-defer.md", "---\nid: DR-X-001\nstatus: proposed\n---\n")
        self.assertFails("deferral: docs/decisions/DR-X-001-defer.md is not accepted")
        self.entry["deferred"]["decision"] = "DR-X-404"
        self.save()
        self.assertFails("no Decision Record DR-X-404")

    def test_a_release_refuses_a_deferred_live_level(self):
        self.entry["levels"]["external"] = []
        self.entry["recorded"] = []
        self.entry["deferred"] = {"decision": "DR-X-001", "needs": "a token"}
        self.save()
        self.assertFails("qualified mock-only: DR-X-001 defers its real-API level until a token", "--release")

    def test_a_live_test_no_workflow_runs_fails(self):
        self.write(".github/workflows/live.yml", "jobs: {}\n")
        self.assertFails("whose live tests no workflow runs")

    def test_an_external_test_must_be_a_live_test(self):
        self.entry["levels"]["external"] = ["crates/net/tests/t.rs::component_calls_the_adapter"]
        self.save()
        self.assertFails("is not a `live_` test the live workflow selects")

    def test_a_fixture_carrying_a_credential_header_fails(self):
        self.fixture["exchanges"][0]["request"]["headers"]["Authorization"] = "Bearer x"
        self.save()
        self.assertFails("carries the credential header 'Authorization'")

    def test_a_fixture_recorded_outside_ci_fails(self):
        self.fixture["provenance"]["run"] = "local"
        self.save()
        self.assertFails("provenance.run 'local' is not a CI run")

    def test_a_fixture_no_retained_live_run_wrote_fails(self):
        self.save(retain=False)
        self.write("evidence/run-123456/live_against_the_real_api.json", json.dumps(self.record("passed")))
        self.assertFails("is not the recording of a retained live run")

    def test_an_edited_fixture_is_no_longer_the_recording(self):
        self.fixture["exchanges"][0]["response"]["chunks"][0]["text"] = "data: {\"edited\": true}\n\n"
        raw = json.dumps(self.fixture, indent=1) + "\n"
        self.write("crates/net/tests/fixtures/x.json", raw)
        self.assertFails("is not the recording of a retained live run")

    def test_a_skipped_live_test_is_not_a_pass(self):
        rec = self.record("skipped")
        rec["reason"] = "MODBIT_LIVE_PROVIDERS=1 not set"
        self.assertFails("live_against_the_real_api skipped (MODBIT_LIVE_PROVIDERS=1 not set): a live test "
                         "that did not pass is not proof", "--live", self.live_dir(rec))

    def test_a_live_test_that_left_no_record_did_not_run(self):
        self.assertFails("left no result record", "--live", self.live_dir(None))

    def test_a_live_pass_that_proved_another_integration_fails(self):
        self.assertFails("passed without proving this integration", "--live",
                         self.live_dir(self.record("passed", proven=("something.else",))))

    def test_a_real_service_needs_its_integration_level_and_no_external_one(self):
        self.entry = {"id": "store", "crates": ["net-crate"], "boundary": "real-service",
                      "counterparty": "an object store", "levels": {"integration": []},
                      "recorded": [{"fixture": "x", "replay": "y"}]}
        self.save()
        out = self.assertFails("store: no integration-level test against the real an object store")
        self.assertIn("a real-service integration has no external API level", out)


if __name__ == "__main__":
    unittest.main(verbosity=2)
