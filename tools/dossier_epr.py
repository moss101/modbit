"""Parse the additive EPR authority from numbered documents (standard library only).

This module deliberately pins the sealed EPR surface: exactly the EPR-000..019 triplets, the ten scoped
supersession pairs, gates A-G and their critical task coverage. Extending any of these requires an approved
Decision Record plus a coordinated change to these constants and tools/test_dossier.py in the same reseal
(docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md).
"""
import hashlib
import json
import re
from pathlib import Path

TASK_DOC = "49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md"
QUAL_DOC = "61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md"
PATCH_FILE = "MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md"
PATCH_ID = "MODBIT-PATCH-EPR-2026-09-05"
DECISION_DOC = "05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md"
HANDOFF_DOC = "94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md"
V11_PATCH_FILE = "MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md"
V11_PATCH_ID = "MODBIT-PATCH-EPR-2026-09-05-v1.1"
V11_DECISION_DOC = "06_EPR_V1_1_SUPERSESSION_DECISION.md"
V11_HANDOFF_DOC = "95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md"
V11_CHANGE = "DR-EPR-2026-09-05-v1.1"
EXPECTED_IDS = {"%03d" % n for n in range(20)}
SUPERSESSION_PAIRS = {(49, 39), (49, 42), (50, 41), (50, 47), (51, 41),
                      (52, 46), (53, 43), (54, 42), (55, 48), (56, 44)}


def supersessions(docs):
    text = (Path(docs) / "02_AUTHORITY_AND_DECISIONS.md").read_text(encoding="utf-8")
    rows = re.findall(r"^\| `(ADR-R-\d{3})` \| `(ADR-R-\d{3})` \| ([^|]+) \|$", text, re.M)
    pairs = {(int(a[-3:]), int(b[-3:])) for a, b, scope in rows}
    if pairs != SUPERSESSION_PAIRS or len(rows) != len(SUPERSESSION_PAIRS):
        raise ValueError("expected exact v1.1 scoped supersession links")
    return [(a, b, scope.strip()) for a, b, scope in rows]


def validate_v11_authority(docs):
    """Validate machine-checkable contract/provenance invariants, not prose truth."""
    docs = Path(docs)
    baseline = json.loads((docs.parent / "evidence/dossier-epr-v1.1/baseline.json").read_text())["files"]
    for path in (PATCH_FILE, V11_PATCH_FILE, "docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md"):
        if hashlib.sha256((docs.parent / path).read_bytes()).hexdigest() != baseline[path]:
            raise ValueError("immutable source hash mismatch: " + path)
    text = (docs / "38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md").read_text()
    contracts = {}
    for line in text.splitlines():
        if line.startswith("| "):
            cells = [c.strip() for c in line.strip("|").split("|")]
            if len(cells) == 2:
                contracts[cells[0]] = cells[1]
    required = {
        "ConditionalExecutionPlan": ("statistics/compiler/gate/realized-risk versions", "prevalidated", "max_total_attempts/max_revisions"),
        "ModelProfile": ("no solver/escalation/reviewer/revision empirical statistics",),
        "OutcomeStatistics": ("stats_version", "sample counts", "independently versioned"),
        "RealizedRisk": ("minimum_assurance", "independent_review_required", "human_required"),
        "AcceptanceGateResult": ("ACCEPT/REJECT/INCONCLUSIVE", "required_assurance", "missing_evidence"),
        "RoutingDecisionRecord / RoutingTrace": ("LCBs", "QUALITY_FLOOR_INFEASIBLE", "executed slot/leg path"),
        "OutcomeRecord": ("separate request/leg/gate outcomes", "no credit to a failed initial leg"),
        "ReviewerResult": ("candidate revision",),
    }
    for name, fields in required.items():
        if name not in contracts or any(field not in contracts[name] for field in fields):
            raise ValueError("v1.1 contract missing required fields: " + name)
    for legacy in ("ExecutionPlan", "QualityGateResult", "CriticResult"):
        if legacy in contracts:
            raise ValueError("legacy contract cannot be a canonical writer: " + legacy)
    headings = set(re.findall(r"^## (.+)$", text, re.M))
    required_algorithms = {"CompileConfidenceFeasiblePlan", "ValidateConditionalExecutionPlan", "ExecuteConditionalTransaction",
                           "ActivateContinuation", "ExecuteIsolatedReview", "DeriveRealizedRisk", "EvaluateAcceptanceGate", "MaterializeOutcomeStatistics"}
    if not required_algorithms <= headings or headings & {"ExecuteDirect", "ExecuteCascade", "ExecuteCritique"}:
        raise ValueError("v1.1 conditional algorithm surface is missing or superseded")
    supersessions(docs)


def table_rows(text, prefix, width):
    rows = []
    for line in text.splitlines():
        if line.startswith("| " + prefix):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) != width or any(not c for c in cells):
                raise ValueError("malformed %s row: %s" % (prefix, line[:120]))
            rows.append(cells)
    ids = [r[0] for r in rows]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate %s row" % prefix)
    return rows


def parse(docs):
    docs = Path(docs)
    validate_v11_authority(docs)
    task_text = (docs / TASK_DOC).read_text(encoding="utf-8")
    qual_text = (docs / QUAL_DOC).read_text(encoding="utf-8")
    headings = re.findall(r"^## (EPR-\d{3}) — (.+)$", task_text, re.M)
    if len(headings) != len(EXPECTED_IDS) or {x[0][4:] for x in headings} != EXPECTED_IDS:
        raise ValueError("expected exactly EPR-000..019 task cards")
    titles = dict(headings)
    requirements, tasks, quals, scenarios, gates = [], [], [], [], []
    rows = table_rows(task_text, "REQ-EPR-", 9)
    if {r[0][8:] for r in rows} != EXPECTED_IDS:
        raise ValueError("expected exactly REQ-EPR-000..019")
    for rid, tid, qid, owner, milestone, phase, after, sections, behavior in rows:
        suffix = rid[8:]
        if tid != "EPR-" + suffix or qid != "QUAL-EPR-" + suffix:
            raise ValueError("mismatched EPR requirement/task/qualification: " + rid)
        body = re.search(r"^## " + tid + r" — .*?\n(.*?)(?=^## |\Z)", task_text, re.M | re.S).group(1)
        requirements.append(dict(id=rid, type="requirement", title=titles[tid], disposition="ADOPT",
                                 subsystem=owner, owner_label=owner, mandatory_behavior=behavior,
                                 imp=tid, qual=qid, source="docs/" + TASK_DOC, patch_sections=sections.split(",")))
        tasks.append(dict(id=tid, type="imp_task", title=titles[tid], requirement=rid, qual=qid,
                          subsystem=owner, owner_label=owner, milestone=milestone, phase=phase,
                          disposition="ADOPT", source="docs/" + TASK_DOC, acceptance=behavior,
                          prerequisites=after.split(","),
                          related_requirements=sorted(set(re.findall(r"REQ-EV-\d{4}", body))),
                          task_card="docs/" + TASK_DOC + "#" + tid.lower(),
                          audit_at_adoption="DOCUMENTED-ONLY; no product caller or executor present"))
    for qid, rid, owner, qualification, failure in table_rows(qual_text, "QUAL-EPR-", 5):
        quals.append(dict(id=qid, type="qual_test", requirement=rid, owner_label=owner,
                          title=qualification, failure_proof=failure,
                          evidence_class="real-system or production-equivalent", source="docs/" + QUAL_DOC))
    if {q["id"][9:] for q in quals} != EXPECTED_IDS:
        raise ValueError("expected exactly QUAL-EPR-000..019")
    qmap = {q["id"]: q for q in quals}
    for task in tasks:
        q = qmap[task["qual"]]
        if q["requirement"] != task["requirement"] or q["owner_label"] != task["subsystem"]:
            raise ValueError("qualification owner/requirement mismatch: " + task["id"])
    for sid, title in re.findall(r"^### (EPR-(?:E2E|FI)-\d{3}) — (.+)$", qual_text, re.M):
        scenarios.append(dict(id=sid, type="scenario", kind="epr-fault" if "-FI-" in sid else "epr-e2e",
                              title=title, source="docs/" + QUAL_DOC))
    expected_scenarios = {"EPR-%s-%s" % (kind, n) for kind in ("E2E", "FI") for n in EXPECTED_IDS}
    if len(scenarios) != 2 * len(EXPECTED_IDS) or {s["id"] for s in scenarios} != expected_scenarios:
        raise ValueError("expected 20 EPR-E2E and 20 EPR-FI scenarios")
    for gid, title, required, acceptance in table_rows(qual_text, "EPR-GATE-", 4):
        reqs = required.split(",")
        if any(t not in titles for t in reqs) or len(reqs) != len(set(reqs)):
            raise ValueError("invalid gate prerequisites: " + gid)
        gates.append(dict(id=gid, type="release_gate", title=title, required_tasks=reqs,
                          acceptance=acceptance, source="docs/" + QUAL_DOC))
    if {g["id"] for g in gates} != {"EPR-GATE-" + c for c in "ABCDEFG"}:
        raise ValueError("expected EPR-GATE-A..G")
    gate_map = {g["id"]: set(g["required_tasks"]) for g in gates}
    critical_tasks = {"A": {"EPR-014", "EPR-017"}, "C": {"EPR-015", "EPR-016"},
                      "D": {"EPR-019"}, "E": {"EPR-018", "EPR-019"}, "G": {"EPR-019"}}
    for suffix, required in critical_tasks.items():
        if not required <= gate_map["EPR-GATE-" + suffix]:
            raise ValueError("v1.1 critical gate coverage missing: EPR-GATE-" + suffix)
    return requirements, tasks, quals, scenarios, gates
