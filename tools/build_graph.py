#!/usr/bin/env python3
"""Build graph/project-graph.json from the dossier.

Structure (nodes/edges) is derived from docs/ every run. Live state on work items
(status, evidence, notes, owner_agent, status_changed_on) is preserved from the
existing graph file by node id.

Usage: python3 tools/build_graph.py
Standard library only, Python 3.9+.
"""
import hashlib
import json
import os
import re
import sys
from datetime import date

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")
OUT = os.path.join(ROOT, "graph", "project-graph.json")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import graph as graphtool  # noqa: E402
import dossier_epr as epr  # noqa: E402
import dossier_px as px  # noqa: E402

SECTIONS = [
    ("authority", 0, 9, "Authority and orientation"),
    ("architecture", 10, 29, "Architecture and subsystems"),
    ("implementation", 30, 39, "Implementation specifications"),
    ("requirements", 40, 49, "Requirements, tasks and traceability"),
    ("verification", 50, 69, "Verification and testing"),
    ("delivery", 70, 79, "Delivery and operations"),
    ("governance", 80, 97, "Agent process and governance"),
    ("live-state", 98, 99, "Live state"),
]

# ---------------------------------------------------------------------------
# Milestones (from docs/43) + dependencies (from its "Critical path" section)
# ---------------------------------------------------------------------------
MILESTONE_DEPS = {
    "M0": [], "M1": ["M0"], "M2": ["M1"], "M3": ["M2"], "M4": ["M2"], "M5": ["M2"],
    "M6": ["M2", "M4"], "M7": ["M2"], "M8": ["M4", "M7"], "M9": ["M4", "M5"],
    "M10": ["M3", "M5", "M6", "M7", "M8", "M9"],
}
CRITICAL_PATH = ["M0", "M1", "M2", "M4"]
GOV_HANDOFF_DOC = "96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md"
GOV_CHANGE = "DR-GOV-2026-09-05"
GOV_LOG_DOC = "97_DOSSIER_MAINTENANCE_LOG.md"
GOV2_CHANGE = "DR-GOV-2026-09-05-002"
GOV3_CHANGE = "DR-GOV-2026-09-05-003"
GOV4_CHANGE = "DR-GOV-2026-09-05-004"
PX_STAGES = [("DOC-PX-001", "DOC-GOV-004", "Product extension stage A: authority, PX ledger tooling, phased releases, governance tiering"),
             ("DOC-PX-002", "DOC-PX-001", "Product extension stage B: client surfaces and source-control integration"),
             ("DOC-PX-003", "DOC-PX-002", "Product extension stage C: agent competence contracts and benchmarks"),
             ("DOC-PX-004", "DOC-PX-003", "Product extension stage D: UX flows, onboarding and interaction budgets"),
             ("DOC-PX-005", "DOC-PX-004", "Product extension stage E: language and platform support matrix"),
             ("DOC-PX-006", "DOC-PX-005", "Product extension stage F: verification execution mechanics, scope bounds, repair policy and agent harness contracts")]
# DR-PX-2026-09-05-006 (docs/97): competence hardening. PX rows from PX6_FIRST_ROW on are authorized by it as well as by DR-PX.
PX6_CHANGE = "DR-PX-2026-09-05-006"
PX6_FIRST_ROW = 32
# IMP-EV milestones derive from OWNER_MAP; the entries below are the recorded exceptions, each justified by a Decision Record.
# Docs 40/41/42 stay byte-identical; the override is stored on the node as `milestone_override` (docs/74).
MILESTONE_OVERRIDES = {
    "IMP-EV-0107": ("M2", "DR-PX-2026-09-05-006: bounded failure evidence is a prerequisite of the M2 repair loop (PX-018, PX-033, PX-039); scheduled ahead of its owner label's default milestone"),
    "IMP-EV-0242": ("M4", "DR-M0-005: QUAL-EV-0242 (restart loses no durable truth while the hook process resets) needs the durable store, hook bus and kill-point recovery suite of M1/M4; scheduled after its owner label's default milestone"),
}

# Tasks named in docs/43's "V2 sequencing delta" but never enumerated as Mx.y rows.
ADDED_TASKS = [
    ("M0", "M0.4", "Requirement-coverage CI and REQ→IMP→QUAL traceability parser",
     "CI fails on ADOPT/ADAPT row without owner/IMP-EV/QUAL-EV, COMPLETE task without evidence, or duplicate active owner",
     "docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md; docs/43 V2 sequencing delta"),
    ("M2", "M2.10", "MediaEnvelope + Media Pipeline (before any multimodal provider/tool feature)",
     "real PNG/JPEG/text-PDF read through fs.read with provenance, budgets and artifact digests",
     "docs/25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md; docs/43 V2 sequencing delta"),
    ("M5", "M5.7", "Skill Evolution Lab as shadow/EXPERIMENT behind Skill Registry + Eval Harness",
     "WSK-E2E-001..010; candidate cannot self-promote; production recovery independent of lab data",
     "docs/26_SKILL_REGISTRY_AND_EVOLUTION.md; docs/57_SKILL_EVOLUTION_REAL_TESTS.md"),
    ("M6", "M6.7", "Durable subagent continuation (background child survives restart)",
     "kill Core mid-child run; child identity, lineage, event offsets and result envelope survive",
     "docs/25 (subagent continuation); docs/43 V2 sequencing delta"),
    ("M10", "M10.7", "Canonical tool and capability conformance harness",
     "every production tool family passes its real-substrate conformance suite; no canned success",
     "docs/56_TOOL_CAPABILITY_CONFORMANCE.md; docs/43 V2 sequencing delta"),
]

# ---------------------------------------------------------------------------
# Canonical subsystems (single-owner boundaries) with crates, spec docs, primary milestone
# ---------------------------------------------------------------------------
SUBSYSTEMS = [
    ("governance", "Architecture Governance & Product Scope", ["tools/architecture-lint", "tools/evidence-check", "docs/decisions"], ["02", "03", "76", "81", "82"], "M0"),
    ("domain-events", "Domain Model, Event Store & Protocol State", ["crates/domain", "crates/protocol", "crates/event-store", "crates/protocol-state"], ["13", "30", "31"], "M1"),
    ("core-runtime", "Agent Runtime, Scheduler, WorkGraph/AgentGraph", ["crates/core-runtime"], ["14", "28"], "M1"),
    ("desktop", "Desktop Surface & UI", ["apps/desktop", "apps/cli", "packages/ui", "packages/surface-protocol", "packages/ide-adapter-core", "packages/design-tokens"], ["10", "29", "32", "39"], "M1"),
    ("model-gateway", "Execution Policy Router & Provider Gateway", ["crates/providers"], ["15", "27", "38"], "M2"),
    ("tool-runtime", "Tool Registry & Capability Kernel", ["crates/tools", "crates/policy"], ["16", "17"], "M2"),
    ("workspace-git", "Workspace Fabric, Change Engine & Git", ["crates/workspace", "crates/git"], ["20", "64"], "M2"),
    ("terminal", "Terminal Broker & Execution Router", ["crates/terminal", "services/modbit-execd"], ["21"], "M2"),
    ("verification", "Verification Engine & Quality Gates", ["crates/verification", "tools/release-gate"], ["28", "50", "51", "63", "64", "76", "83"], "M2"),
    ("context-engine", "Context Engine, Retrieval & Diagnostics", ["crates/context", "crates/retrieval", "crates/diagnostics"], ["18", "28", "76"], "M3"),
    ("durability", "Compaction, Checkpoints & Recovery Spine", ["crates/compaction", "crates/checkpoint"], ["19"], "M4"),
    ("procedural-runtime", "Procedural Tool Runtime", ["crates/procedural-runtime"], ["16"], "M5"),
    ("skills", "Skill Registry, Compiler & Evolution Lab", ["crates/skills", "crates/prompt-compiler"], ["26"], "M5"),
    ("media", "Media Pipeline & Artifact Store", ["crates/tools (media)", "object store"], ["25"], "M5"),
    ("eval-bench", "Eval Harness & Benchmarks", ["benchmarks/retrieval", "benchmarks/context-economics", "benchmarks/agent-engineering", "benchmarks/latency"], ["53", "61", "63"], "M3"),
    ("browser", "Browser & Computer Runtime", ["crates/browser"], ["22"], "M7"),
    ("sandbox-cloud", "Sandbox Gateway, Guest & Cloud Control Plane", ["crates/sandbox", "apps/cloud-api", "apps/cloud-worker", "apps/sandbox-gateway", "services/modbit-guest"], ["21", "24"], "M8"),
    ("effects-security", "Policy Kernel, Effect Ledger & Secrets", ["crates/policy", "crates/effects", "crates/secrets"], ["23", "52"], "M9"),
    ("memory", "Engineering Memory", ["crates/memory"], ["19"], "M9"),
    ("external-tools", "MCP Hub, Integrations & Web Gateway", ["crates/tools (external.*)", "crates/tools (forge.*)"], ["16", "29"], "M9"),
    ("extensions-hooks", "Hook Bus, Extension System & Importers", ["crates/tools (hooks)", "crates/skills (import)"], ["25"], "M9"),
    ("observability", "Observability, Cost & Operations", ["crates/observability"], ["34", "71"], "M10"),
    ("automation", "Automation / Scheduling (DEFERRED)", [], ["02"], None),
]

# Ledger owner label -> (subsystem id, milestone id or None)
OWNER_MAP = {
    "Agent Runtime": ("core-runtime", "M6"), "Agent/Task Runtime": ("core-runtime", "M6"),
    "Agent Admission": ("core-runtime", "M6"), "Agent Profile Registry": ("core-runtime", "M6"),
    "Agent/Tool Profiles": ("core-runtime", "M6"), "Coordinator + TaskContract": ("core-runtime", "M6"),
    "Task Isolation Bundle": ("core-runtime", "M6"), "Resource Governor": ("core-runtime", "M6"),
    "WorkGraph": ("core-runtime", "M6"), "Scheduler": ("core-runtime", "M1"), "Task Runtime": ("core-runtime", "M1"),
    "Input Queue": ("core-runtime", "M1"), "Input Gateway": ("core-runtime", "M1"),
    "Parallel Change Coordinator": ("workspace-git", "M6"),
    "Attention Manager": ("desktop", "M6"), "WorkGraph UI": ("desktop", "M6"), "Workspace UI": ("desktop", "M1"),
    "Workspace UX": ("desktop", "M2"), "Workspace UI + Change Engine": ("desktop", "M2"),
    "Workspace Browser Surface": ("browser", "M7"), "Desktop Security": ("desktop", "M1"),
    "Context Engine": ("context-engine", "M3"), "Context Graph": ("context-engine", "M3"),
    "Context Economy": ("context-engine", "M3"), "Context Query Planner": ("context-engine", "M3"),
    "Context Pack Compiler": ("context-engine", "M3"), "Context Inspector": ("context-engine", "M3"),
    "Context Policy": ("context-engine", "M3"), "Context Connectors": ("context-engine", "M3"),
    "Context/Policy": ("context-engine", "M3"), "Context + Session Stores": ("context-engine", "M3"),
    "Context + Change Engine": ("context-engine", "M3"), "Repository Index": ("context-engine", "M3"),
    "Repository Knowledge": ("context-engine", "M3"), "Workspace Context Bridge": ("context-engine", "M3"),
    "Diagnostics Adapter": ("context-engine", "M3"), "Session Index + Engineering Memory": ("context-engine", "M3"),
    "Context Eval": ("eval-bench", "M3"), "Benchmark Harness": ("eval-bench", "M3"), "Benchmark Method": ("eval-bench", "M3"),
    "Eval Harness": ("eval-bench", "M5"), "Eval Registry": ("eval-bench", "M5"), "Learning/Eval": ("eval-bench", "M10"),
    "Adaptive Profile Evaluator": ("eval-bench", "M10"), "Qualification Suite": ("verification", "M10"),
    "Quality Gate": ("verification", "M2"), "Verification Plane": ("verification", "M2"),
    "Workspace Fabric": ("workspace-git", "M2"), "Change Engine": ("workspace-git", "M2"),
    "Worktree Manager": ("workspace-git", "M2"), "Policy + Workspace Fabric": ("effects-security", "M2"),
    "Terminal Broker": ("terminal", "M2"), "ExecutionBackend": ("sandbox-cloud", "M8"),
    "Policy Kernel": ("effects-security", "M2"), "Capability Kernel": ("effects-security", "M2"),
    "Policy Profiles": ("effects-security", "M2"), "Effect Ledger": ("effects-security", "M9"),
    "Secret Broker": ("effects-security", "M9"), "Approval Service": ("tool-runtime", "M2"),
    "Approval/Question Service": ("tool-runtime", "M2"), "Tool Runtime": ("tool-runtime", "M2"),
    "Tool Registry": ("tool-runtime", "M2"), "Procedural Tool Runtime": ("procedural-runtime", "M5"),
    "Skill Registry": ("skills", "M5"), "Skill Compiler": ("skills", "M5"), "Skill Package": ("skills", "M5"),
    "Skill/Tool Developer Kit": ("skills", "M5"), "Skill Registry + Eval": ("skills", "M5"),
    "Skill Evolution Lab": ("skills", "M5"), "Instruction Compiler": ("skills", "M5"), "Instruction + Memory": ("skills", "M5"),
    "Model Gateway": ("model-gateway", "M2"), "Model Router": ("model-gateway", "M2"), "Provider Adapter": ("model-gateway", "M2"),
    "Session Store": ("domain-events", "M1"), "Domain Model": ("domain-events", "M1"), "Persistence": ("domain-events", "M1"),
    "Event Protocol": ("domain-events", "M1"), "Transport": ("domain-events", "M1"), "Core API": ("domain-events", "M1"),
    "Configuration Service": ("domain-events", "M1"), "Execution Timeline": ("domain-events", "M1"),
    "Protocol Store": ("domain-events", "M4"), "Checkpoint Store": ("durability", "M4"), "Reliability Layer": ("durability", "M4"),
    "Artifact Store": ("domain-events", "M2"), "OutputRef": ("domain-events", "M2"),
    "Evidence Archive": ("domain-events", "M2"), "Evidence Index": ("domain-events", "M2"),
    "CLI/API Surface": ("domain-events", "M10"), "External Client Adapter": ("domain-events", "M10"),
    "Media Pipeline": ("media", "M5"), "Media + File Tool": ("media", "M5"), "MCP Hub + Media": ("media", "M5"),
    "Artifact/Notebook Adapter": ("media", "M5"),
    "Browser Runtime": ("browser", "M7"), "Computer Runtime": ("browser", "M7"), "Semantic Browser Compiler": ("browser", "M7"),
    "Browser Event Protocol": ("browser", "M7"), "Browser/Computer Runtime": ("browser", "M7"),
    "Computer Runtime + Evidence": ("browser", "M7"), "MCP/Browser Gateway": ("browser", "M7"),
    "Sandbox Gateway": ("sandbox-cloud", "M8"), "SandboxBackend": ("sandbox-cloud", "M8"), "Sandbox Policy": ("sandbox-cloud", "M8"),
    "Guest RPC": ("sandbox-cloud", "M8"), "Worker Fabric": ("sandbox-cloud", "M8"), "Worker Protocol": ("sandbox-cloud", "M8"),
    "Worker Gateway": ("sandbox-cloud", "M8"), "Core + Worker Fabric": ("sandbox-cloud", "M8"),
    "Enterprise Networking": ("sandbox-cloud", "M8"),
    "Engineering Memory": ("memory", "M9"),
    "MCP Hub": ("external-tools", "M9"), "Integration Broker": ("external-tools", "M9"),
    "Hook Bus": ("extensions-hooks", "M9"), "Extension System": ("extensions-hooks", "M9"), "Importers": ("extensions-hooks", "M9"),
    "Usage Ledger": ("observability", "M10"), "Observability": ("observability", "M10"), "Error Service": ("observability", "M10"),
    "Operations": ("observability", "M10"),
    "Core subsystems": ("governance", "M0"), "Core owners": ("governance", "M0"), "Core Architecture": ("governance", "M0"),
    "Architecture Governance": ("governance", "M0"), "Clean-room governance": ("governance", "M0"), "Product Scope": ("governance", "M0"),
    "Automation": ("automation", None),
}

E2E_MILESTONE = {
    "E2E-001": "M2", "E2E-002": "M2", "E2E-003": "M2", "E2E-004": "M4", "E2E-005": "M4", "E2E-006": "M4",
    "E2E-007": "M4", "E2E-008": "M4", "E2E-009": "M6", "E2E-010": "M6", "E2E-011": "M5", "E2E-012": "M5",
    "E2E-013": "M7", "E2E-014": "M7", "E2E-015": "M7", "E2E-016": "M7", "E2E-017": "M8", "E2E-018": "M8",
    "E2E-019": "M2", "E2E-020": "M2", "E2E-021": "M2", "E2E-022": "M2", "E2E-023": "M9", "E2E-024": "M8", "E2E-025": "M10",
}

DECISION_SUBSYSTEM = {
    "PROD": "governance", "SURF": "desktop", "UX": "desktop", "DESK": "desktop", "IDE": "governance",
    "CORE": "core-runtime", "ORCH": "core-runtime", "AGENT": "core-runtime", "INPUT": "core-runtime",
    "STATE": "durability", "TOOL": "tool-runtime", "EXEC": "terminal", "EFFECT": "effects-security",
    "BROWSE": "browser", "SBX": "sandbox-cloud", "CLOUD": "sandbox-cloud", "AUTH": "sandbox-cloud",
    "CTX": "context-engine", "EMB": "context-engine", "VERIFY": "verification", "COV": "governance",
    "MEDIA": "media", "MM": "media", "SKILL": "skills", "JIT": "eval-bench", "MOBILE": "automation", "AUTO": "automation",
}

NODE_TYPES = {
    "section": "numbering range of the dossier",
    "doc": "one specification file in docs/",
    "milestone": "M0–M10 from docs/43; carries proof statement and dependency edges",
    "milestone_task": "Mx.y row from docs/43 (plus five tasks the V2 sequencing delta named but did not enumerate); ordered inside its milestone; carries status",
    "subsystem": "canonical single-owner boundary (docs/81); owns REQ rows and IMP tasks; delivered in a primary milestone",
    "requirement": "REQ-EV from docs/40, additive REQ-EPR from docs/49 or additive REQ-PX from docs/62",
    "imp_task": "IMP-EV from docs/41, EPR from docs/49 or PX from docs/62; carries status and evidence",
    "dossier_task": "governance package work outside product milestone roll-ups",
    "release_gate": "EPR promotion gate from docs/61; derived state OPEN/TASKS_COMPLETE/SATISFIED; carries attestation evidence, never a lifecycle status",
    "source_patch": "immutable user-supplied patch provenance",
    "change_record": "explicit approved dossier amendment",
    "qual_test": "QUAL-EV from docs/42, QUAL-EPR from docs/61 or QUAL-PX from docs/62",
    "release": "ALPHA / BETA / RELEASE_ZERO projection from docs/75; readiness derived from included work items and required gates, never stored",
    "scenario": "E2E-nnn, WSK-E2E-nnn, MEDIA-E2E-nnn, EPR-E2E/FI-nnn or PX-E2E-nnn scenario, or FI-nn fault case",
    "decision": "MOD-* or ADR-R-* decision from docs/02 with authority status",
}
EDGE_TYPES = {
    "in_section": "doc → section",
    "references": "doc → doc (explicit filename mention)",
    "depends_on": "milestone → milestone it requires COMPLETE first",
    "part_of": "milestone_task → milestone",
    "after": "work item → required COMPLETE work item, including cross-milestone EPR dependencies",
    "delivered_in": "subsystem → primary milestone",
    "specified_by": "subsystem → doc",
    "owned_by_req": "requirement → subsystem",
    "owned_by": "imp_task → subsystem",
    "scheduled_in": "imp_task → milestone",
    "implemented_by": "requirement → imp_task",
    "qualified_by": "requirement → qual_test",
    "proven_by": "imp_task → qual_test",
    "proves": "scenario → milestone",
    "constrains": "decision → subsystem",
    "requires_task": "release gate → required implementation task",
    "extends": "additive requirement → related preserved EV requirement",
    "authorized_by": "adopted task/decision/requirement → approved change record",
    "adopts": "change record → immutable source patch",
    "supersedes": "new authority → prior authority, only within the recorded scope",
    "refines": "v1.1 source/change → previous source/change; non-conflicting authority survives",
    "gated_by": "milestone → release gate that must be SATISFIED before the milestone rolls up COMPLETE",
    "includes": "release → product work item whose COMPLETE status the release requires",
    "requires_gate": "release → release gate that must be SATISFIED before the release is READY",
}


def read(name):
    with open(os.path.join(DOCS, name), encoding="utf-8") as fh:
        return fh.read()


def docs_list():
    return sorted(f for f in os.listdir(DOCS) if f.endswith(".md"))


def doc_by_number(n):
    for f in docs_list():
        if f.startswith("%s_" % n):
            return f
    sys.exit("no doc with number %s" % n)


def title_of(text, fallback):
    for line in text.splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return fallback


def build(previous=None):
    """Derive structure from current documents, preserving only supplied live state."""
    prev = {n["id"]: n for n in (previous or {}).get("nodes", [])}

    nodes, edges = [], []

    def add(n):
        nodes.append(n)

    def link(a, b, t):
        edges.append({"from": a, "to": b, "type": t})

    # sections + docs -------------------------------------------------------
    for i, (key, lo, hi, title) in enumerate(SECTIONS):
        add({"id": "section:" + key, "type": "section", "title": title, "range": "%02d-%02d" % (lo, hi), "order": i})
    all_docs = docs_list()
    doc_text = {f: read(f) for f in all_docs}
    for f in all_docs:
        n = int(f[:2])
        sec = [k for k, lo, hi, t in SECTIONS if lo <= n <= hi][0]
        add({"id": f, "type": "doc", "title": title_of(doc_text[f], f), "path": "docs/" + f, "number": n, "section": sec})
        link(f, "section:" + sec, "in_section")
    for f in all_docs:
        for ref in sorted(set(re.findall(r"\b(\d{2}_[A-Z0-9_]+\.md)\b", doc_text[f]))):
            if ref != f and ref in doc_text:
                link(f, ref, "references")

    # milestones + tasks from docs/43 --------------------------------------
    roadmap = read(doc_by_number("43"))
    cur = None
    m_order, t_order = 0, {}
    milestones, mtasks = {}, []
    for line in roadmap.splitlines():
        mh = re.match(r"^## (M\d+) — (.+?)(?: \((P\d[^)]*)\))?$", line.strip())
        if mh:
            cur = mh.group(1)
            milestones[cur] = {"id": cur, "type": "milestone", "title": mh.group(2).strip(), "priority": mh.group(3) or "",
                               "order": m_order, "proof": "", "source": "docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md"}
            m_order += 1
            t_order[cur] = 0
            continue
        if cur is None:
            continue
        th = re.match(r"^\*\*(M\d+\.\d+)\*\*\s+(.+?)\s*$", line.strip())
        if th:
            tid, rest = th.group(1), th.group(2)
            acc = ""
            m2 = re.match(r"^(.*?)\s*Acceptance:\s*(.*)$", rest)
            if m2:
                rest, acc = m2.group(1).strip(), m2.group(2).strip()
            mtasks.append({"id": tid, "type": "milestone_task", "title": rest.rstrip("."), "acceptance": acc,
                           "milestone": cur, "order": t_order[cur], "source": "docs/43"})
            t_order[cur] += 1
            continue
        ph = re.match(r"^(?:Milestone proof|Proof):\s*(.+)$", line.strip())
        if ph:
            milestones[cur]["proof"] = ph.group(1).strip()
        ah = re.match(r"^Acceptance:\s*(.+)$", line.strip())
        if ah and mtasks and mtasks[-1]["milestone"] == cur and not mtasks[-1]["acceptance"]:
            mtasks[-1]["acceptance"] = ah.group(1).strip()
    if len(milestones) != 11:
        sys.exit("expected 11 milestones in docs/43, found %d" % len(milestones))
    # docs/98 carries a "Required proof" column; use it where docs/43 has no proof line
    for line in read(doc_by_number("98")).splitlines():
        h = re.match(r"^\| (M\d+) \| (.+?) \| [A-Z_]+ \| (.+?) \|$", line.strip())
        if h and h.group(1) in milestones:
            milestones[h.group(1)]["scope"] = h.group(2).strip()
            if not milestones[h.group(1)]["proof"]:
                milestones[h.group(1)]["proof"] = h.group(3).strip()
    for mid, mt_id, title, acc, src in ADDED_TASKS:
        mtasks.append({"id": mt_id, "type": "milestone_task", "title": title, "acceptance": acc, "milestone": mid,
                       "order": t_order[mid], "source": src, "note": "added in V3.1 from the V2 sequencing delta"})
        t_order[mid] += 1
    for mid in milestones:
        add(milestones[mid])
    for mid, deps in MILESTONE_DEPS.items():
        for d in deps:
            link(mid, d, "depends_on")
    by_ms = {}
    for t in mtasks:
        add(t)
        link(t["id"], t["milestone"], "part_of")
        by_ms.setdefault(t["milestone"], []).append(t)
    for mid, ts in by_ms.items():
        ts.sort(key=lambda x: x["order"])
        for a, b in zip(ts, ts[1:]):
            link(b["id"], a["id"], "after")

    # subsystems -------------------------------------------------------------
    for sid, title, crates, docnums, ms in SUBSYSTEMS:
        add({"id": sid, "type": "subsystem", "title": title, "crates": crates, "primary_milestone": ms})
        if ms:
            link(sid, ms, "delivered_in")
        for dn in docnums:
            link(sid, doc_by_number(dn), "specified_by")

    # requirements / tasks / tests -----------------------------------------
    ledger = read(doc_by_number("40"))
    reqs = {}
    for line in ledger.splitlines():
        if not line.startswith("| REQ-EV-"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 8:
            sys.exit("malformed ledger row: %s" % line[:80])
        rid, mech, disp, owner, behav, imp, qual, realq = cells[:8]
        reqs[rid] = {"id": rid, "type": "requirement", "title": mech, "disposition": disp, "owner_label": owner,
                     "mandatory_behavior": behav, "imp": imp if imp.startswith("IMP-EV-") else None,
                     "qual": qual, "real_qualification": realq}
    tasks_text = read(doc_by_number("41"))
    imps = {}
    owner = None
    for line in tasks_text.splitlines():
        oh = re.match(r"^## (.+)$", line)
        if oh:
            owner = oh.group(1).strip()
            continue
        th = re.match(r"^### (IMP-EV-\d{4}) — (.+)$", line)
        if th:
            imps[th.group(1)] = {"id": th.group(1), "type": "imp_task", "title": th.group(2).strip(), "owner_label": owner}
    quals = {}
    for line in read(doc_by_number("42")).splitlines():
        if line.startswith("| QUAL-EV-"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            quals[cells[0]] = {"id": cells[0], "type": "qual_test", "requirement": cells[1], "owner_label": cells[3],
                               "title": cells[4], "evidence_class": cells[5] if len(cells) > 5 else ""}

    unknown_owners = sorted(set(r["owner_label"] for r in reqs.values()) - set(OWNER_MAP))
    if unknown_owners:
        sys.exit("OWNER_MAP is missing: %s" % unknown_owners)

    for rid in sorted(reqs):
        r = reqs[rid]
        sub, ms = OWNER_MAP[r["owner_label"]]
        n = dict(r)
        n["subsystem"] = sub
        n["source"] = "docs/40"
        add(n)
        link(rid, sub, "owned_by_req")
        if r["imp"]:
            if r["imp"] not in imps:
                sys.exit("%s names %s but docs/41 has no such task" % (rid, r["imp"]))
            link(rid, r["imp"], "implemented_by")
        if r["qual"] in quals:
            link(rid, r["qual"], "qualified_by")
        else:
            sys.exit("%s names %s but docs/42 has no such test" % (rid, r["qual"]))
    for iid in sorted(imps):
        t = imps[iid]
        rid = "REQ-EV-" + iid[-4:]
        r = reqs.get(rid)
        if not r:
            sys.exit("%s has no matching %s" % (iid, rid))
        sub, ms = OWNER_MAP[r["owner_label"]]
        n = dict(t)
        if iid in MILESTONE_OVERRIDES:
            ms, n["milestone_override"] = MILESTONE_OVERRIDES[iid]
        n.update({"requirement": rid, "disposition": r["disposition"], "subsystem": sub, "milestone": ms,
                  "qual": r["qual"], "source": "docs/41"})
        add(n)
        link(iid, sub, "owned_by")
        if ms:
            link(iid, ms, "scheduled_in")
        link(iid, r["qual"], "proven_by")
    for qid in sorted(quals):
        add(dict(quals[qid], source="docs/42"))

    # scenarios ---------------------------------------------------------------
    for line in read(doc_by_number("51")).splitlines():
        h = re.match(r"^## (E2E-\d{3}) — (.+)$", line)
        if h:
            add({"id": h.group(1), "type": "scenario", "kind": "e2e", "title": h.group(2).strip(), "source": "docs/51"})
            link(h.group(1), E2E_MILESTONE[h.group(1)], "proves")
    for docnum, kind, ms in (("57", "skill-evolution", "M5"), ("58", "media", "M5")):
        for line in read(doc_by_number(docnum)).splitlines():
            h = re.match(r"^## ((?:WSK|MEDIA)-E2E-\d{3}) — (.+)$", line)
            if h:
                add({"id": h.group(1), "type": "scenario", "kind": kind, "title": h.group(2).strip(), "source": "docs/" + docnum})
                link(h.group(1), ms, "proves")
    fault_ms = [("browser", "M7"), ("human takes", "M7"), ("sandbox", "M8"), ("cloud", "M8"), ("subagent", "M6"),
                ("MCP", "M9"), ("secret", "M9"), ("terminal", "M2"), ("context index", "M3")]
    for line in read(doc_by_number("54")).splitlines():
        h = re.match(r"^(\d{1,2})\. (.+?)\.?$", line.strip())
        if h and int(h.group(1)) <= 30:
            fid = "FI-%02d" % int(h.group(1))
            ms = "M4"
            for kw, m in fault_ms:
                if kw.lower() in h.group(2).lower():
                    ms = m
                    break
            add({"id": fid, "type": "scenario", "kind": "fault", "title": h.group(2).strip(), "source": "docs/54"})
            link(fid, ms, "proves")

    # decisions -------------------------------------------------------------
    for line in read(doc_by_number("02")).splitlines():
        h = re.match(r"^\| (MOD-([A-Z]+)-\d{3}) \| (.+?) \| \*\*(.+?)\*\* \| (.+?) \|$", line.strip())
        if h:
            did, fam, title, status, cons = h.groups()
            if status.strip() not in graphtool.DECISION_STATES:
                raise ValueError("decision status outside docs/93 ladder: %s is %r" % (did, status.strip()))
            add({"id": did, "type": "decision", "title": title.strip(), "status": status.strip(),
                 "consequence": cons.strip(), "source": "docs/02"})
            sub = DECISION_SUBSYSTEM.get(fam)
            if sub:
                link(did, sub, "constrains")

    # Additive EPR authority and traceability (no new canonical subsystem) ----
    ereqs, etasks, equals, escenarios, egates = epr.parse(DOCS)
    subsystem_ids = {s[0] for s in SUBSYSTEMS}
    for n in ereqs + etasks:
        if n["subsystem"] not in subsystem_ids:
            raise ValueError("unknown EPR canonical owner: " + n["subsystem"])
    for n in etasks:
        if n["milestone"] not in milestones:
            raise ValueError("unknown EPR milestone: " + n["milestone"])
    for n in ereqs + etasks + equals + escenarios + egates:
        add(n)
    for r in ereqs:
        link(r["id"], r["subsystem"], "owned_by_req")
        link(r["id"], r["imp"], "implemented_by")
        link(r["id"], r["qual"], "qualified_by")
        link(r["id"], epr.TASK_DOC, "specified_by")
        if int(r["id"][-3:]) < 14:
            link(r["id"], "DR-EPR-2026-09-05", "authorized_by")
        link(r["id"], epr.V11_CHANGE, "authorized_by")
    for t in etasks:
        link(t["id"], epr.V11_CHANGE, "authorized_by")
        link(t["id"], t["subsystem"], "owned_by")
        link(t["id"], t["milestone"], "scheduled_in")
        link(t["id"], t["qual"], "proven_by")
        link(t["id"], epr.TASK_DOC, "specified_by")
        for dep in t["prerequisites"]:
            link(t["id"], dep, "after")
        for rid in t["related_requirements"]:
            link(t["requirement"], rid, "extends")
        for kind in ("E2E", "FI"):
            sid = "EPR-%s-%s" % (kind, t["id"][-3:])
            link(t["id"], sid, "proven_by")
            link(sid, t["milestone"], "proves")
    for gate in egates:
        link(gate["id"], epr.QUAL_DOC, "specified_by")
        link("M10", gate["id"], "gated_by")
        for tid in gate["required_tasks"]:
            link(gate["id"], tid, "requires_task")
            link(gate["id"], "QUAL-" + tid, "proven_by")
    link("M10.3", "EPR-013", "after")
    link("M10.3", "EPR-019", "after")

    # Additive product-extension ledger (DR-PX-2026-09-05); totals computed from rows -------
    preqs, ptasks, pquals, pscen = px.parse(DOCS)
    for n in preqs + ptasks:
        if n["subsystem"] not in subsystem_ids:
            raise ValueError("unknown PX canonical owner: " + n["subsystem"])
    for n in ptasks:
        if n["milestone"] not in milestones:
            raise ValueError("unknown PX milestone: " + n["milestone"])
    add({"id": px.CHANGE, "type": "change_record", "title": "Approved product extension: additive PX ledger, phased releases, governance tiering",
         "status": "APPROVED", "source": "docs/" + px.DECISION_DOC})
    link(px.CHANGE, px.DECISION_DOC, "specified_by")
    add({"id": PX6_CHANGE, "type": "change_record", "title": "Approved competence hardening: verification execution mechanics, scope bounds, repair policy and agent harness contracts",
         "status": "APPROVED", "source": "docs/" + GOV_LOG_DOC})
    link(PX6_CHANGE, GOV_LOG_DOC, "specified_by")
    link(PX6_CHANGE, px.CHANGE, "refines")
    for n in preqs + ptasks + pquals + pscen:
        add(n)
    for r in preqs:
        link(r["id"], r["subsystem"], "owned_by_req")
        link(r["id"], r["qual"], "qualified_by")
        link(r["id"], px.LEDGER_DOC, "specified_by")
        link(r["id"], px.CHANGE, "authorized_by")
        if int(r["id"][-3:]) >= PX6_FIRST_ROW:
            link(r["id"], PX6_CHANGE, "authorized_by")
        if r["imp"]:
            link(r["id"], r["imp"], "implemented_by")
    for t in ptasks:
        link(t["id"], px.CHANGE, "authorized_by")
        if int(t["id"][-3:]) >= PX6_FIRST_ROW:
            link(t["id"], PX6_CHANGE, "authorized_by")
        link(t["id"], t["subsystem"], "owned_by")
        link(t["id"], t["milestone"], "scheduled_in")
        link(t["id"], t["qual"], "proven_by")
        link(t["id"], px.LEDGER_DOC, "specified_by")
        for dep in t["prerequisites"]:
            link(t["id"], dep, "after")
        for rid in t["related_requirements"]:
            link(t["requirement"], rid, "extends")
        sid = "PX-E2E-" + t["id"][-3:]
        link(t["id"], sid, "proven_by")
        link(sid, t["milestone"], "proves")
    for sid in ("domain-events", "core-runtime", "effects-security", "verification", "skills", "observability", "eval-bench"):
        link(sid, doc_by_number("38"), "specified_by")
    link("verification", epr.QUAL_DOC, "specified_by")
    patch_path = os.path.join(ROOT, epr.PATCH_FILE)
    with open(patch_path, "rb") as fh:
        patch_hash = hashlib.sha256(fh.read()).hexdigest()
    add({"id": epr.PATCH_ID, "type": "source_patch", "title": "Execution policy source patch v1.0",
         "path": epr.PATCH_FILE, "sha256": patch_hash})
    add({"id": "DR-EPR-2026-09-05", "type": "change_record", "title": "Approved EPR dossier adoption",
         "status": "APPROVED", "source": "docs/" + epr.DECISION_DOC})
    link("DR-EPR-2026-09-05", epr.PATCH_ID, "adopts")
    link("DR-EPR-2026-09-05", epr.DECISION_DOC, "specified_by")
    add({"id": epr.V11_PATCH_ID, "type": "source_patch", "title": "EPR v1.1 supersession and refinement source",
         "path": epr.V11_PATCH_FILE, "sha256": hashlib.sha256(open(os.path.join(ROOT, epr.V11_PATCH_FILE), "rb").read()).hexdigest()})
    add({"id": epr.V11_CHANGE, "type": "change_record", "title": "Approved scoped EPR v1.1 supersession",
         "status": "APPROVED", "source": "docs/" + epr.V11_DECISION_DOC})
    link(epr.V11_CHANGE, epr.V11_PATCH_ID, "adopts")
    link(epr.V11_CHANGE, epr.V11_DECISION_DOC, "specified_by")
    link(epr.V11_CHANGE, "DR-EPR-2026-09-05", "refines")
    link(epr.V11_PATCH_ID, epr.PATCH_ID, "refines")
    adr_owners = {39: ["model-gateway"], 40: ["model-gateway"], 41: ["model-gateway"],
                  42: ["core-runtime"], 43: ["effects-security", "core-runtime"], 44: ["eval-bench"],
                  45: ["observability"], 46: ["effects-security"], 47: ["core-runtime", "model-gateway"],
                  48: ["observability", "core-runtime"], 49: ["core-runtime", "model-gateway"],
                  50: ["model-gateway"], 51: ["eval-bench"], 52: ["effects-security", "verification"],
                  53: ["effects-security", "core-runtime"], 54: ["core-runtime"],
                  55: ["observability"], 56: ["eval-bench", "verification"]}
    adr_rows = epr.table_rows(read(doc_by_number("02")), "ADR-R-", 4)
    if {r[0] for r in adr_rows} != {"ADR-R-%03d" % n for n in range(39, 57)}:
        raise ValueError("expected ADR-R-039..056")
    for did, title, status, consequence in adr_rows:
        if status != "**LOCKED**":
            raise ValueError("EPR adopted decision must remain LOCKED: " + did)
        add({"id": did, "type": "decision", "title": title, "status": "LOCKED",
             "consequence": consequence, "source": "docs/02"})
        if int(did[-3:]) < 49:
            link(did, "DR-EPR-2026-09-05", "authorized_by")
        link(did, epr.V11_CHANGE, "authorized_by")
        for owner in adr_owners[int(did[-3:])]:
            link(did, owner, "constrains")
    add({"id": "DOC-EPR-001", "type": "dossier_task", "title": "Adopt execution policy patch into development dossier",
         "subsystem": "governance", "source": "docs/" + epr.HANDOFF_DOC,
         "acceptance": "Current source/graph/manifest equivalence and copied-package negative tests; no product proof claim"})
    link("DOC-EPR-001", "governance", "owned_by")
    link("DOC-EPR-001", epr.HANDOFF_DOC, "specified_by")
    link("DOC-EPR-001", "DR-EPR-2026-09-05", "authorized_by")

    for newer, earlier, scope in epr.supersessions(DOCS):
        edges.append({"from": newer, "to": earlier, "type": "supersedes", "scope": scope})
    add({"id": "DOC-EPR-002", "type": "dossier_task", "title": "Integrate and reseal EPR v1.1 supersession",
         "subsystem": "governance", "source": "docs/" + epr.V11_HANDOFF_DOC,
         "acceptance": "Current scoped authority, task/graph/manifest equivalence, immutable sources and real copied-package negative tests; no product proof"})
    link("DOC-EPR-002", "DOC-EPR-001", "after")
    link("DOC-EPR-002", "governance", "owned_by")
    link("DOC-EPR-002", epr.V11_HANDOFF_DOC, "specified_by")
    link("DOC-EPR-002", epr.V11_CHANGE, "authorized_by")
    add({"id": GOV_CHANGE, "type": "change_record", "title": "Approved governing-file, evidence-grammar and decision-status maintenance",
         "status": "APPROVED", "source": "docs/" + GOV_HANDOFF_DOC})
    link(GOV_CHANGE, GOV_HANDOFF_DOC, "specified_by")
    link("MOD-SKILL-001", GOV_CHANGE, "authorized_by")
    add({"id": "DOC-GOV-001", "type": "dossier_task", "title": "Reseal governing files, evidence grammar and decision statuses",
         "subsystem": "governance", "source": "docs/" + GOV_HANDOFF_DOC,
         "acceptance": "Governing files match the package and five-step reseal; evidence references validated; decision statuses in vocabulary; pinned tool constants documented; copied-package negative tests pass; no product proof"})
    link("DOC-GOV-001", "DOC-EPR-002", "after")
    link("DOC-GOV-001", "governance", "owned_by")
    link("DOC-GOV-001", GOV_HANDOFF_DOC, "specified_by")
    link("DOC-GOV-001", GOV_CHANGE, "authorized_by")
    add({"id": GOV2_CHANGE, "type": "change_record", "title": "Approved alignment of implementation specifications with EPR v1.1",
         "status": "APPROVED", "source": "docs/" + GOV_LOG_DOC})
    link(GOV2_CHANGE, GOV_LOG_DOC, "specified_by")
    link(GOV2_CHANGE, epr.V11_CHANGE, "refines")
    add({"id": "DOC-GOV-002", "type": "dossier_task", "title": "Align implementation specs and execution profiles with EPR v1.1",
         "subsystem": "governance", "source": "docs/" + GOV_LOG_DOC,
         "acceptance": "Docs 12/14/16/21/33/44 carry v1.1 placement, review_isolated profile and reviewer projection with no superseded wording; copied-package tests pass; no product proof"})
    link("DOC-GOV-002", "DOC-GOV-001", "after")
    link("DOC-GOV-002", "governance", "owned_by")
    link("DOC-GOV-002", GOV_LOG_DOC, "specified_by")
    link("DOC-GOV-002", GOV2_CHANGE, "authorized_by")
    add({"id": GOV3_CHANGE, "type": "change_record", "title": "Approved release-gate attestation and one-step lifecycle enforcement",
         "status": "APPROVED", "source": "docs/" + GOV_LOG_DOC})
    link(GOV3_CHANGE, GOV_LOG_DOC, "specified_by")
    add({"id": "DOC-GOV-003", "type": "dossier_task", "title": "Enforce release-gate attestation and one-step lifecycle transitions",
         "subsystem": "governance", "source": "docs/" + GOV_LOG_DOC,
         "acceptance": "gated_by edges, derived gate states, attest/gates commands, GATED roll-up, one-step transitions with noted BLOCKED/backward moves, G7 check; copied-package tests pass; no product proof"})
    link("DOC-GOV-003", "DOC-GOV-002", "after")
    link("DOC-GOV-003", "governance", "owned_by")
    link("DOC-GOV-003", GOV_LOG_DOC, "specified_by")
    link("DOC-GOV-003", GOV3_CHANGE, "authorized_by")
    add({"id": GOV4_CHANGE, "type": "change_record", "title": "Approved product-facing integration of both EPR patches and source coverage map",
         "status": "APPROVED", "source": "docs/" + GOV_LOG_DOC})
    link(GOV4_CHANGE, GOV_LOG_DOC, "specified_by")
    link(GOV4_CHANGE, epr.V11_CHANGE, "refines")
    add({"id": "DOC-GOV-004", "type": "dossier_task", "title": "Integrate EPR patches into product, cloud, durability and acceptance docs; add coverage map",
         "subsystem": "governance", "source": "docs/" + GOV_LOG_DOC,
         "acceptance": "Docs 10/17/19/24/32/56/83 carry the patch UX, cloud, durability, projection, conformance and done rules; doc 27 coverage map covers every patch section and D8 enforces it; copied-package tests pass; no product proof"})
    link("DOC-GOV-004", "DOC-GOV-003", "after")
    link("DOC-GOV-004", "governance", "owned_by")
    link("DOC-GOV-004", GOV_LOG_DOC, "specified_by")
    link("DOC-GOV-004", GOV4_CHANGE, "authorized_by")
    for stage_id, prev_stage, stage_title in PX_STAGES:
        add({"id": stage_id, "type": "dossier_task", "title": stage_title, "subsystem": "governance", "source": "docs/" + GOV_LOG_DOC,
             "acceptance": "Stage sealed on main with change and seal commits; full integrity gate passes; no product proof"})
        link(stage_id, prev_stage, "after")
        link(stage_id, "governance", "owned_by")
        link(stage_id, GOV_LOG_DOC, "specified_by")
        link(stage_id, px.CHANGE, "authorized_by")
        if stage_id == "DOC-PX-006":
            link(stage_id, PX6_CHANGE, "authorized_by")

    # releases: derived projections over work items (docs/75) ----------------
    ms_of, owner_of = {}, {}
    for e in edges:
        if e["type"] in ("part_of", "scheduled_in"):
            ms_of[e["from"]] = e["to"]
        elif e["type"] == "owned_by":
            owner_of[e["from"]] = e["to"]
    px_release = {t["id"]: t["release"] for t in ptasks}
    work_items = [n for n in nodes if n["type"] in ("milestone_task", "imp_task")]
    for order, rule in enumerate(px.release_rules(DOCS)):
        add({"id": rule["id"], "type": "release", "title": rule["title"], "order": order, "rule": rule["rule"],
             "source": "docs/" + px.RELEASE_DOC})
        for w in work_items:
            wid = w["id"]
            if wid in px_release:
                included = rule["all"] or graphtool.RELEASE_ORDER.index(px_release[wid]) <= order
            else:
                included = rule["all"] or (ms_of.get(wid) in rule["milestones"] and wid not in rule["exclude_tasks"]
                                           and not any(wid.startswith(pfx) for pfx in rule["exclude_prefixes"])
                                           and owner_of.get(wid) not in rule["exclude_owners"])
            if included:
                link(rule["id"], wid, "includes")
        for gid in rule["gates"]:
            link(rule["id"], gid, "requires_gate")

    # live state preservation ------------------------------------------------
    for n in nodes:
        if n["type"] in graphtool.WORK_TYPES:
            old = prev.get(n["id"], {})
            n["status"] = old.get("status", "NOT_STARTED")
            n["evidence"] = old.get("evidence", [])
            for k in ("notes", "owner_agent", "status_changed_on", "blocked_from"):
                if k in old:
                    n[k] = old[k]
        elif n["type"] == "release_gate":
            old = prev.get(n["id"], {})
            n["evidence"] = old.get("evidence", [])
            for k in ("attested_on", "attested_by", "notes"):
                if k in old:
                    n[k] = old[k]

    ids = [n["id"] for n in nodes]
    if len(ids) != len(set(ids)):
        dup = sorted(set(i for i in ids if ids.count(i) > 1))
        sys.exit("duplicate node ids: %s" % dup[:10])
    idset = set(ids)
    for e in edges:
        if e["from"] not in idset or e["to"] not in idset:
            sys.exit("dangling edge: %s" % e)

    g = {
        "schema_version": "1.2",
        "product": "Modbit",
        "edition": "V3.3 EPR v1.1",
        "authority_date": "2026-09-05",
        "generated_on": date.today().isoformat(),
        "generator": "tools/build_graph.py",
        "status_vocabulary": graphtool.STATES,
        "status_rules": "docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md; E2E_PROVEN and COMPLETE require evidence; COMPLETE requires all 'after' prerequisites COMPLETE",
        "critical_path": CRITICAL_PATH,
        "node_types": NODE_TYPES,
        "edge_types": EDGE_TYPES,
        "nodes": nodes,
        "edges": edges,
    }
    return g


def main():
    previous = None
    if os.path.exists(OUT):
        with open(OUT, encoding="utf-8") as fh:
            previous = json.load(fh)
    try:
        g = build(previous)
    except (ValueError, OSError) as exc:
        sys.exit(str(exc))
    cycles = graphtool.dependency_findings(graphtool.Index(g))
    if cycles:
        sys.exit("invalid dependencies: " + "; ".join(cycles))
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as fh:
        json.dump(g, fh, indent=1, ensure_ascii=False)
        fh.write("\n")
    with open(graphtool.VIEW_PATH, "w", encoding="utf-8") as fh:
        fh.write(graphtool.render(g))
    from collections import Counter
    c = Counter(n["type"] for n in g["nodes"])
    print("graph written: %d nodes, %d edges" % (len(g["nodes"]), len(g["edges"])))
    print("  " + ", ".join("%s=%d" % kv for kv in sorted(c.items())))
    print("view written: graph/PROJECT_GRAPH.md")


if __name__ == "__main__":
    main()
