#!/usr/bin/env python3
"""Integrity gate for the Modbit dossier, graph and manifest.

Usage: python3 tools/check_dossier.py [--manifest]
Exit code 0 = clean, 1 = findings. Standard library only, Python 3.9+.

Checks
  D1  every doc has a unique two-digit number prefix
  D2  every `NN_*.md` reference in docs/ and root files resolves
  D3  exactly 291 unique REQ-EV rows; 265 IMP-EV; 291 QUAL-EV; IDs consistent
  D4  every ADOPT/ADAPT/EXPERIMENT row has an IMP-EV task and a QUAL-EV test
  D5  no de-branding artifacts or placeholder tokens
  D7  decision statuses in docs/02 and on graph decision nodes use the docs/93 decision ladder
  G1  graph exists and every doc is a node; every doc node exists on disk
  G2  every REQ/IMP/QUAL in docs is in the graph and vice versa
  G3  statuses are from the vocabulary; E2E_PROVEN/COMPLETE carry evidence; every evidence ref follows the docs/93 grammar
  G4  COMPLETE tasks have COMPLETE prerequisites; edges resolve
  G5  docs/98 milestone table matches the graph roll-up (GATED is a valid roll-up state)
  G7  release gates: attestation evidence grammar; no attestation while required tasks are incomplete; gated_by edges valid
  D6/G6 additive EPR coverage; current source/graph equivalence; acyclic prerequisites
  M1  (--manifest) every manifest.json hash matches the file on disk
"""
import hashlib
import json
import os
import re
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import graph as graphtool  # noqa: E402
import dossier_epr as epr  # noqa: E402
import build_graph as graphbuilder  # noqa: E402
import build_manifest as manifestbuilder  # noqa: E402

EXPECTED = {"REQ": 291, "IMP": 265, "QUAL": 291}
ARTIFACTS = ["selected MicroVM", "validated multimodal mechanisms", "validated CLI-agent", "validated agent mechanisms",
             "substrate substrate", "skill-evolution research", "tool/skill packaging research", "`SQ-*`"]
PLACEHOLDERS = [r"\bTBD\b", r"\bUNREVIEWED\b", r"TBD IMPLEMENTATION"]

findings = []


def find(code, msg):
    findings.append((code, msg))


def read(p):
    with open(p, encoding="utf-8") as fh:
        return fh.read()


def main(argv):
    findings.clear()
    docs = sorted(f for f in os.listdir(DOCS) if f.endswith(".md"))
    text = {f: read(os.path.join(DOCS, f)) for f in docs}

    # D1
    nums = Counter()
    for f in docs:
        m = re.match(r"^(\d{2})_", f)
        if not m:
            find("D1", "doc without numeric prefix: %s" % f)
        else:
            nums[m.group(1)] += 1
    for n, c in nums.items():
        if c > 1:
            find("D1", "number %s used %d times" % (n, c))

    # D2
    roots = ["README.md", "AGENTS.md", "SKILLS.md", "docs/98_BUILD_MANIFEST.md"]
    for f in docs:
        for ref in set(re.findall(r"\b(\d{2}_[A-Z0-9_]+\.md)\b", text[f])):
            if ref not in text:
                find("D2", "%s references missing %s" % (f, ref))
    for r in roots:
        p = os.path.join(ROOT, r)
        if not os.path.exists(p):
            find("D2", "missing root file %s" % r)
            continue
        for ref in set(re.findall(r"\b(\d{2}_[A-Z0-9_]+\.md)\b", read(p))):
            if ref not in text:
                find("D2", "%s references missing %s" % (r, ref))

    # D3 / D4
    ledger = [f for f in docs if "REQUIREMENT_LEDGER" in f][0]
    tasks = [f for f in docs if "IMPLEMENTATION_TASKS" in f][0]
    quals = [f for f in docs if "QUALIFICATION_TEST_MATRIX" in f][0]
    req_rows = [l for l in text[ledger].splitlines() if l.startswith("| REQ-EV-")]
    req_ids = [l.split("|")[1].strip() for l in req_rows]
    if len(req_ids) != EXPECTED["REQ"] or len(set(req_ids)) != EXPECTED["REQ"]:
        find("D3", "expected %d unique REQ-EV rows, found %d (%d unique)" % (EXPECTED["REQ"], len(req_ids), len(set(req_ids))))
    imp_ids = set(re.findall(r"^### (IMP-EV-\d{4})", text[tasks], re.M))
    if len(imp_ids) != EXPECTED["IMP"]:
        find("D3", "expected %d IMP-EV tasks, found %d" % (EXPECTED["IMP"], len(imp_ids)))
    qual_ids = set(l.split("|")[1].strip() for l in text[quals].splitlines() if l.startswith("| QUAL-EV-"))
    if len(qual_ids) != EXPECTED["QUAL"]:
        find("D3", "expected %d QUAL-EV tests, found %d" % (EXPECTED["QUAL"], len(qual_ids)))
    for l in req_rows:
        cells = [c.strip() for c in l.strip().strip("|").split("|")]
        rid, disp, imp, qual = cells[0], cells[2], cells[5], cells[6]
        if disp in ("ADOPT", "ADAPT", "EXPERIMENT"):
            if not imp.startswith("IMP-EV-"):
                find("D4", "%s (%s) has no IMP-EV task" % (rid, disp))
            elif imp not in imp_ids:
                find("D4", "%s names %s which is not in %s" % (rid, imp, tasks))
        if qual not in qual_ids:
            find("D4", "%s names %s which is not in %s" % (rid, qual, quals))
    for iid in imp_ids:
        if "REQ-EV-" + iid[-4:] not in req_ids:
            find("D4", "%s has no matching requirement row" % iid)

    e_req_ids, e_imp_ids, e_qual_ids = set(), set(), set()
    try:
        er, et, eq, es, eg = epr.parse(DOCS)
        e_req_ids = {n["id"] for n in er}
        e_imp_ids = {n["id"] for n in et}
        e_qual_ids = {n["id"] for n in eq}
    except (ValueError, OSError) as exc:
        find("D6", "EPR coverage invalid: " + str(exc))

    # D5
    for f in docs + roots:
        t = text.get(f) or read(os.path.join(ROOT, f))
        for a in ARTIFACTS:
            if a in t:
                find("D5", "%s contains artifact phrase %r" % (f, a))
        if f.endswith(("46_REQUIREMENT_COVERAGE_FREEZE_GATE.md", "74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md",
                       "82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md", "check_dossier.py")):
            continue
        for pat in PLACEHOLDERS:
            if re.search(pat, t):
                find("D5", "%s contains placeholder token %s" % (f, pat))

    # D7: decision statuses are single values from the docs/93 ladder
    decisions_doc = [f for f in docs if f.startswith("02_")][0]
    for did, status in re.findall(r"^\| ((?:MOD-[A-Z]+|ADR-R)-\d{3}) \| .+? \| \*\*(.+?)\*\* \| .+? \|$", text[decisions_doc], re.M):
        if status.strip() not in graphtool.DECISION_STATES:
            find("D7", "%s has decision status %r outside %s" % (did, status.strip(), "/".join(graphtool.DECISION_STATES)))

    # graph
    adr_count = gate_count = 0
    gp = graphtool.GRAPH_PATH
    if not os.path.exists(gp):
        find("G1", "graph missing: run tools/build_graph.py")
    else:
        g = json.load(open(gp, encoding="utf-8"))
        ix = graphtool.Index(g)
        if len(ix.nodes) != len(g["nodes"]):
            find("G6", "duplicate graph node IDs")
        for message in graphtool.dependency_findings(ix):
            find("G6", message)
        doc_nodes = {n["id"] for n in ix.by_type("doc")}
        for f in docs:
            if f not in doc_nodes:
                find("G1", "doc not in graph: %s (run tools/build_graph.py)" % f)
        for d in doc_nodes:
            if d not in text:
                find("G1", "graph doc node has no file: %s" % d)
        g_req = {n["id"] for n in ix.by_type("requirement")}
        g_imp = {n["id"] for n in ix.by_type("imp_task")}
        g_qual = {n["id"] for n in ix.by_type("qual_test")}
        for label, a, b in (("REQ", set(req_ids) | e_req_ids, g_req),
                            ("IMP", imp_ids | e_imp_ids, g_imp), ("QUAL", qual_ids | e_qual_ids, g_qual)):
            if a != b:
                find("G2", "%s mismatch docs vs graph: docs-only=%s graph-only=%s" % (label, sorted(a - b)[:5], sorted(b - a)[:5]))
        adr_count = sum(1 for n in g["nodes"] if n["type"] == "decision" and n["id"].startswith("ADR-R-"))
        gate_count = sum(1 for n in g["nodes"] if n["type"] == "release_gate")
        for n in g["nodes"]:
            if n["type"] == "release_gate":
                for ref in n.get("evidence") or []:
                    err = graphtool.evidence_ref_error(ref)
                    if err:
                        find("G7", "%s: %s" % (n["id"], err))
                if n.get("evidence") and ix.gate_state(n["id"]) == "OPEN":
                    find("G7", "%s carries attestation evidence while required tasks are not COMPLETE" % n["id"])
            if n["type"] == "decision" and n.get("status") not in graphtool.DECISION_STATES:
                find("D7", "graph decision %s has status %r outside the docs/93 ladder" % (n["id"], n.get("status")))
            if n["type"] in graphtool.WORK_TYPES:
                st = n.get("status", "NOT_STARTED")
                if st not in graphtool.STATES:
                    find("G3", "%s has invalid status %r" % (n["id"], st))
                if st in graphtool.EVIDENCE_REQUIRED and not n.get("evidence"):
                    find("G3", "%s is %s without evidence" % (n["id"], st))
                for ref in n.get("evidence") or []:
                    err = graphtool.evidence_ref_error(ref)
                    if err:
                        find("G3", "%s: %s" % (n["id"], err))
                if st == "COMPLETE":
                    for d in ix.outs(n["id"], "after"):
                        if ix.nodes.get(d, {}).get("status") != "COMPLETE":
                            find("G4", "%s is COMPLETE but prerequisite %s is %s" % (n["id"], d, ix.nodes.get(d, {}).get("status")))
                    mid = ix.milestone_of(n["id"])
                    if mid and not ix.milestone_unblocked(mid):
                        find("G4", "%s is COMPLETE but upstream milestones are incomplete" % n["id"])
        for e in g["edges"]:
            if e["from"] not in ix.nodes or e["to"] not in ix.nodes:
                find("G4", "dangling edge %s" % e)
        # G5: docs/98 table vs roll-up
        bm = [f for f in docs if "BUILD_MANIFEST" in f][0]
        roll = {r["id"]: r["state"] for r in graphtool.rollup(ix)}
        for l in text[bm].splitlines():
            m = re.match(r"^\| (M\d+) \| .+? \| ([A-Z_]+) \|", l)
            if m:
                mid, st = m.groups()
                if roll.get(mid) != st:
                    find("G5", "docs/98 says %s %s but graph roll-up is %s" % (mid, st, roll.get(mid)))
                if st not in graphtool.STATES and st not in ("IN_PROGRESS", graphtool.MILESTONE_GATED):
                    find("G5", "docs/98 row %s uses non-vocabulary status %s" % (mid, st))

        try:
            expected_graph = graphbuilder.build(g)
            if g["nodes"] != expected_graph["nodes"] or g["edges"] != expected_graph["edges"]:
                find("G6", "graph structure differs from current dossier; run tools/build_graph.py")
            for key in ("schema_version", "edition", "authority_date", "node_types", "edge_types", "critical_path"):
                if g.get(key) != expected_graph[key]:
                    find("G6", "stale graph metadata: " + key)
            if os.path.exists(graphtool.VIEW_PATH) and read(graphtool.VIEW_PATH) != graphtool.render(g):
                find("G6", "human graph view is stale; regenerate graph or render --write")
        except (ValueError, OSError, KeyError, SystemExit) as exc:
            find("G6", "cannot derive graph from dossier: " + str(exc))

    # M1
    if "--manifest" in argv:
        mp = os.path.join(ROOT, "manifest.json")
        if not os.path.exists(mp):
            find("M1", "manifest.json missing: run tools/build_manifest.py")
        else:
            man = json.load(open(mp, encoding="utf-8"))
            entries = man["docs"] + man["root_and_tooling"]
            paths = [e["path"] for e in entries]
            if len(paths) != len(set(paths)):
                find("M1", "duplicate manifest paths")
            expected_paths = {"docs/" + f for f in docs} | set(manifestbuilder.package_files())
            if set(paths) != expected_paths:
                find("M1", "manifest coverage mismatch: missing=%s extra=%s" % (
                    sorted(expected_paths - set(paths)), sorted(set(paths) - expected_paths)))
            if man.get("edition") != manifestbuilder.EDITION or man.get("authority_date") != manifestbuilder.AUTHORITY_DATE:
                find("M1", "manifest edition/authority metadata is stale")
            human_manifest = os.path.join(ROOT, "MANIFEST.md")
            if not os.path.exists(human_manifest) or read(human_manifest) != manifestbuilder.render(man):
                find("M1", "MANIFEST.md differs from its machine-readable twin")
            expected_counts = {"docs": len(man["docs"]), "root_and_tooling": len(man["root_and_tooling"]),
                               "docs_bytes": sum(e["bytes"] for e in man["docs"])}
            if man.get("counts") != expected_counts:
                find("M1", "manifest summary counts are inconsistent")
            for e in entries:
                p = os.path.join(ROOT, e["path"])
                if not os.path.exists(p):
                    find("M1", "manifest lists missing file %s" % e["path"])
                    continue
                h = hashlib.sha256(open(p, "rb").read()).hexdigest()
                if h != e["sha256"]:
                    find("M1", "hash mismatch %s (run tools/build_manifest.py)" % e["path"])
                if os.path.getsize(p) != e["bytes"]:
                    find("M1", "size mismatch " + e["path"])
            listed = {e["path"] for e in man["docs"]}
            for f in docs:
                if "docs/" + f not in listed:
                    find("M1", "doc not in manifest: %s" % f)

    if findings:
        for code, msg in findings:
            print("[%s] %s" % (code, msg))
        print("FAIL: %d finding(s)" % len(findings))
        return 1
    print("OK: %d docs, %d REQ-EV, %d IMP-EV, %d QUAL-EV + %d REQ-EPR/EPR/QUAL-EPR, %d ADR-R, %d gates; graph and references consistent%s" % (
        len(docs), len(req_ids), len(imp_ids), len(qual_ids), len(e_req_ids), adr_count, gate_count,
        "; manifest hashes verified" if "--manifest" in argv else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
