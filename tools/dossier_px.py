"""Parse the additive product-extension ledger (REQ-PX / PX / QUAL-PX) and the phased release rules.

Standard library only. Unlike the sealed EPR ledger this parser pins no count: effective totals are computed
from the rows, and what it enforces is structural consistency, which is what DR-PX-2026-09-05 requires:
contiguous IDs from 000, one task card and one PX-E2E scenario per ADOPT row, one qualification per row with
the same owner, DEFERRED rows without task/milestone/prerequisites, and release rules that nest.
"""
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import graph as graphtool  # noqa: E402

LEDGER_DOC = "62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md"
DECISION_DOC = "07_PRODUCT_EXTENSION_DECISION_RECORD.md"
RELEASE_DOC = "75_PHASED_RELEASE_PLAN_AND_READINESS.md"
CHANGE = "DR-PX-2026-09-05"
TASK_RELEASES = graphtool.RELEASE_ORDER + ["POST_ZERO"]
DISPOSITIONS = ("ADOPT", "DEFERRED")
NONE = "—"


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


def _split(cell):
    return [] if cell == NONE else [x.strip() for x in cell.split(",") if x.strip()]


def parse(docs):
    """Return (requirements, tasks, quals, scenarios); totals are computed, never pinned."""
    docs = Path(docs)
    text = (docs / LEDGER_DOC).read_text(encoding="utf-8")
    rows = table_rows(text, "REQ-PX-", 9)
    if not rows:
        raise ValueError("product extension ledger has no REQ-PX rows")
    suffixes = sorted(r[0][7:] for r in rows)
    if suffixes != ["%03d" % i for i in range(len(rows))]:
        raise ValueError("REQ-PX rows must be contiguous from 000; found %s" % suffixes)
    cards = dict(re.findall(r"^## (PX-\d{3}) — (.+)$", text, re.M))
    scen = dict(re.findall(r"^### (PX-E2E-\d{3}) — (.+)$", text, re.M))
    qrows = {q[0]: q for q in table_rows(text, "QUAL-PX-", 5)}
    requirements, tasks, quals, scenarios = [], [], [], []
    for rid, title, tid, qid, owner, milestone, release, disposition, after in rows:
        s = rid[7:]
        if qid != "QUAL-PX-" + s or qid not in qrows:
            raise ValueError("missing or mismatched qualification for " + rid)
        q = qrows[qid]
        if q[1] != rid or q[2] != owner:
            raise ValueError("qualification owner/requirement mismatch: " + qid)
        if disposition not in DISPOSITIONS:
            raise ValueError("invalid disposition %r on %s" % (disposition, rid))
        if disposition == "ADOPT":
            if tid != "PX-" + s or not re.match(r"^M\d+$", milestone) or release not in graphtool.RELEASE_ORDER:
                raise ValueError("ADOPT row %s needs task PX-%s, a milestone and a release in %s" % (rid, s, "/".join(graphtool.RELEASE_ORDER)))
            if tid not in cards:
                raise ValueError("missing task card for " + tid)
            sid = "PX-E2E-" + s
            if sid not in scen:
                raise ValueError("missing scenario %s for %s" % (sid, tid))
            body = re.search(r"^## " + tid + r" — .*?\n(.*?)(?=^## |\Z)", text, re.M | re.S).group(1)
            tasks.append(dict(id=tid, type="imp_task", title=cards[tid], requirement=rid, qual=qid, subsystem=owner,
                              owner_label=owner, milestone=milestone, release=release, disposition="ADOPT",
                              source="docs/" + LEDGER_DOC, acceptance=q[3], prerequisites=_split(after),
                              related_requirements=sorted(set(re.findall(r"REQ-EV-\d{4}|REQ-EPR-\d{3}", body))),
                              task_card="docs/" + LEDGER_DOC + "#" + tid.lower(),
                              audit_at_adoption="DOCUMENTED-ONLY; no product caller present"))
            scenarios.append(dict(id=sid, type="scenario", kind="px-e2e", title=scen[sid], source="docs/" + LEDGER_DOC))
        else:
            if tid != NONE or milestone != NONE or release != "POST_ZERO" or after != NONE:
                raise ValueError("DEFERRED row %s must carry no task, milestone or prerequisites and release POST_ZERO" % rid)
            if "PX-" + s in cards or "PX-E2E-" + s in scen:
                raise ValueError("DEFERRED row %s must not have a task card or scenario" % rid)
        requirements.append(dict(id=rid, type="requirement", title=title, disposition=disposition, subsystem=owner,
                                 owner_label=owner, imp=(tid if disposition == "ADOPT" else None), qual=qid, release=release,
                                 source="docs/" + LEDGER_DOC, mandatory_behavior=q[3]))
        quals.append(dict(id=qid, type="qual_test", requirement=rid, owner_label=owner, title=q[3], failure_proof=q[4],
                          evidence_class="real-system or production-equivalent", source="docs/" + LEDGER_DOC))
    extra = set(cards) - {t["id"] for t in tasks}
    if extra:
        raise ValueError("task cards without ADOPT ledger rows: %s" % sorted(extra))
    extra = set(scen) - {s["id"] for s in scenarios}
    if extra:
        raise ValueError("scenarios without ADOPT ledger rows: %s" % sorted(extra))
    if set(qrows) != {q["id"] for q in quals}:
        raise ValueError("qualification rows without ledger rows")
    return requirements, tasks, quals, scenarios


def release_rules(docs):
    """Release membership rules from docs/75; releases are projections, never a status ladder."""
    text = (Path(docs) / RELEASE_DOC).read_text(encoding="utf-8")
    rules = []
    for line in text.splitlines():
        if not re.match(r"^\| (ALPHA|BETA|RELEASE_ZERO) \|", line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 7 or any(not c for c in cells):
            raise ValueError("malformed release rule row: " + line[:120])
        rid, title, milestones, ex_tasks, ex_prefixes, ex_owners, gates = cells
        rules.append(dict(id=rid, title=title, all=(milestones == "ALL"), milestones=[] if milestones == "ALL" else _split(milestones),
                          exclude_tasks=_split(ex_tasks), exclude_prefixes=_split(ex_prefixes), exclude_owners=_split(ex_owners),
                          gates=_split(gates), rule=line.strip()))
    if [r["id"] for r in rules] != graphtool.RELEASE_ORDER:
        raise ValueError("expected release rule rows ALPHA, BETA, RELEASE_ZERO in that order")
    if not rules[-1]["all"]:
        raise ValueError("RELEASE_ZERO must include ALL work items")
    return rules
