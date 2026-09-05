#!/usr/bin/env python3
"""Exercise the real dossier CLIs against disposable packages; no product proof.

Run: python3 tools/test_dossier.py
Uses only Python 3.9+ and the standard library. Negative cases never modify the
working dossier or weaken a production security check.
"""
import hashlib
import json
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
        shutil.copytree(ROOT, self.root, ignore=shutil.ignore_patterns(
            ".git", "__pycache__", ".DS_Store", "node_modules", "target"))

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
        self.edit(TASKS, "| M2 | 1 | EPR-014,EPR-016 |", "| M2 | 1 | M3.9 |")
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
        self.run_tool("graph", "set", "EPR-006", "COMPLETE", ok=False, contains="requires at least one")
        self.run_tool("graph", "set", "EPR-006", "COMPLETE", "--evidence", "run:fixture-test",
                      ok=False, contains="upstream milestones are incomplete")
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

    def test_lifecycle_transitions_are_one_step(self):
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
        self.run_tool("graph", "attest", "EPR-GATE-A", "--evidence", "run:fixture-gate", ok=False, contains="required tasks not COMPLETE")
        self.run_tool("graph", "attest", "EPR-GATE-A", ok=False, contains="at least one --evidence")
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
