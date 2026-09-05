#!/usr/bin/env python3
"""Query and update the Modbit project graph (graph/project-graph.json).

Commands
  ready   [--all]            work items whose dependencies are satisfied
  show    <node-id>          one node with every incoming/outgoing edge
  set     <node-id> <STATE> [--evidence REF]... [--note TEXT] [--agent NAME]
          REF grammar: <kind>:<value>, kind in run|test|commit|revision|build|env|artifact|event|effect|checkpoint
          (docs/93); artifact: package paths must exist. Free text is rejected.
          Transitions move one lifecycle step at a time; BLOCKED and backward moves need --note.
  attest  <gate-id> --evidence REF... [--note TEXT] [--agent NAME]
          record release-gate evidence; refused until every required task is COMPLETE
  gates                      release-gate readiness: OPEN / TASKS_COMPLETE / SATISFIED, and gated milestones
  releases                   derived release readiness (ALPHA / BETA / RELEASE_ZERO): NOT_READY / BLOCKED / READY
  ready --release ALPHA      restrict the ready list to work items included in one release
  status                     milestone roll-up
  render  [--write]          mermaid/markdown view (stdout, or graph/PROJECT_GRAPH.md)
  path                       critical path and milestone dependency order
  stats                      node/edge counts by type

Standard library only, Python 3.9+.
"""
import argparse
import json
import os
import re
import sys
from collections import Counter, defaultdict
from datetime import datetime, timezone

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GRAPH_PATH = os.path.join(ROOT, "graph", "project-graph.json")
VIEW_PATH = os.path.join(ROOT, "graph", "PROJECT_GRAPH.md")

LIFECYCLE = ["NOT_STARTED", "AUDITING", "IMPLEMENTING", "WIRED", "REAL_TESTING", "E2E_PROVEN", "COMPLETE"]
STATES = LIFECYCLE + ["BLOCKED"]
EVIDENCE_REQUIRED = {"E2E_PROVEN", "COMPLETE"}
WORK_TYPES = {"milestone_task", "imp_task", "dossier_task"}
DECISION_STATES = ["LOCKED", "PROVISIONAL", "EXPERIMENT", "DEFERRED", "REJECTED"]
EVIDENCE_KINDS = ["run", "test", "commit", "revision", "build", "env", "artifact", "event", "effect", "checkpoint"]
EVIDENCE_REF = re.compile(r"^(%s):([!-~]+)$" % "|".join(EVIDENCE_KINDS))
PACKAGE_PATH = re.compile(r"^[A-Za-z0-9_./-]+$")
GATE_STATES = ["OPEN", "TASKS_COMPLETE", "SATISFIED"]
MILESTONE_GATED = "GATED"
RELEASE_ORDER = ["ALPHA", "BETA", "RELEASE_ZERO"]
RELEASE_STATES = ["NOT_READY", "BLOCKED", "READY"]


def evidence_ref_error(ref, root=ROOT):
    """Return why an evidence reference violates the docs/93 grammar, or None when it is valid."""
    if not isinstance(ref, str):
        return "evidence reference must be a string: %r" % (ref,)
    m = EVIDENCE_REF.match(ref)
    if not m:
        return ("evidence reference %r must be <kind>:<value> with kind in %s and a non-empty value without "
                "whitespace (docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md)" % (ref, "|".join(EVIDENCE_KINDS)))
    kind, value = m.groups()
    if kind == "artifact" and "/" in value and PACKAGE_PATH.match(value):
        if value.startswith("/") or ".." in value.split("/"):
            return "artifact evidence path must be package-relative: %r" % ref
        if not os.path.isfile(os.path.join(root, value)):
            return "artifact evidence file does not exist in the package: %r" % ref
    return None


def load():
    if not os.path.exists(GRAPH_PATH):
        sys.exit("graph not found: %s (run tools/build_graph.py)" % GRAPH_PATH)
    with open(GRAPH_PATH, encoding="utf-8") as fh:
        return json.load(fh)


def save(g):
    g["updated_on"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    with open(GRAPH_PATH, "w", encoding="utf-8") as fh:
        json.dump(g, fh, indent=1, ensure_ascii=False)
        fh.write("\n")
    with open(VIEW_PATH, "w", encoding="utf-8") as fh:
        fh.write(render(g))


class Index(object):
    def __init__(self, g):
        self.g = g
        self.nodes = {n["id"]: n for n in g["nodes"]}
        self.out = defaultdict(list)
        self.inc = defaultdict(list)
        for e in g["edges"]:
            self.out[e["from"]].append(e)
            self.inc[e["to"]].append(e)

    def outs(self, nid, etype=None):
        return [e["to"] for e in self.out.get(nid, []) if etype is None or e["type"] == etype]

    def ins(self, nid, etype=None):
        return [e["from"] for e in self.inc.get(nid, []) if etype is None or e["type"] == etype]

    def by_type(self, t):
        return [n for n in self.g["nodes"] if n["type"] == t]

    # ---- milestone helpers -------------------------------------------------
    def milestone_of(self, nid):
        n = self.nodes[nid]
        if n["type"] == "milestone":
            return nid
        if n["type"] == "milestone_task":
            return self.outs(nid, "part_of")[0]
        if n["type"] == "imp_task":
            subs = self.outs(nid, "owned_by")
            if not subs:
                return None
            ms = self.outs(nid, "scheduled_in")
            return ms[0] if ms else None
        return None

    def milestone_work(self, mid):
        items = [nid for nid in self.ins(mid, "part_of")]
        items += [nid for nid in self.ins(mid, "scheduled_in")]
        return [self.nodes[i] for i in items if self.nodes[i]["type"] in WORK_TYPES]

    def milestone_state(self, mid):
        work = self.milestone_work(mid)
        if not work:
            return "NOT_STARTED"
        states = [w.get("status", "NOT_STARTED") for w in work]
        if any(s == "BLOCKED" for s in states):
            return "BLOCKED"
        if all(s == "COMPLETE" for s in states):
            if any(self.gate_state(gid) != "SATISFIED" for gid in self.milestone_gates(mid)):
                return MILESTONE_GATED
            return "COMPLETE"
        if all(s == "NOT_STARTED" for s in states):
            return "NOT_STARTED"
        return "IN_PROGRESS"

    def milestone_unblocked(self, mid):
        return all(self.milestone_state(d) == "COMPLETE" for d in self.outs(mid, "depends_on"))

    # ---- release gates ------------------------------------------------------
    def milestone_gates(self, mid):
        return self.outs(mid, "gated_by")

    def gate_state(self, gid):
        """Derived, never stored: OPEN until every required task is COMPLETE, then TASKS_COMPLETE, then SATISFIED once attested."""
        required = self.outs(gid, "requires_task")
        if not required or any(self.nodes[t].get("status") != "COMPLETE" for t in required):
            return "OPEN"
        return "SATISFIED" if self.nodes[gid].get("evidence") else "TASKS_COMPLETE"

    # ---- releases (derived projections, never a stored status) ----------------
    def release_items(self, rid):
        return self.outs(rid, "includes")

    def release_state(self, rid):
        """READY only when every included work item is COMPLETE and every required gate is SATISFIED; BLOCKED if any item is BLOCKED."""
        items = self.release_items(rid)
        statuses = [self.nodes[i].get("status", "NOT_STARTED") for i in items]
        if any(s == "BLOCKED" for s in statuses):
            return "BLOCKED"
        gates = self.outs(rid, "requires_gate")
        if items and all(s == "COMPLETE" for s in statuses) and all(self.gate_state(g) == "SATISFIED" for g in gates):
            return "READY"
        return "NOT_READY"


def dependency_findings(ix):
    """Check executable prerequisites, including implicit milestone roll-ups."""
    adjacency = defaultdict(set)
    errors = []
    for edge in ix.g["edges"]:
        if edge["from"] not in ix.nodes or edge["to"] not in ix.nodes:
            errors.append("dangling edge: %s" % edge)
            continue
        if edge["type"] == "after":
            if any(ix.nodes[edge[k]]["type"] not in WORK_TYPES for k in ("from", "to")):
                errors.append("after edge must join work items: %s" % edge)
            adjacency[edge["from"]].add(edge["to"])
        elif edge["type"] == "depends_on":
            if any(ix.nodes[edge[k]]["type"] != "milestone" for k in ("from", "to")):
                errors.append("depends_on edge must join milestones: %s" % edge)
            adjacency[edge["from"]].add(edge["to"])
        elif edge["type"] == "includes":
            if ix.nodes[edge["from"]]["type"] != "release" or ix.nodes[edge["to"]]["type"] not in ("milestone_task", "imp_task"):
                errors.append("includes edge must join a release to a product work item: %s" % edge)
        elif edge["type"] == "requires_gate":
            if ix.nodes[edge["from"]]["type"] != "release" or ix.nodes[edge["to"]]["type"] != "release_gate":
                errors.append("requires_gate edge must join a release to a release gate: %s" % edge)
        elif edge["type"] == "gated_by":
            if ix.nodes[edge["from"]]["type"] != "milestone" or ix.nodes[edge["to"]]["type"] != "release_gate":
                errors.append("gated_by edge must join a milestone to a release gate: %s" % edge)
    for milestone in ix.by_type("milestone"):
        for task in ix.milestone_work(milestone["id"]):
            adjacency[milestone["id"]].add(task["id"])
            adjacency[task["id"]].update(ix.outs(milestone["id"], "depends_on"))
    visited, active = set(), set()

    def visit(nid, path):
        if nid in active:
            errors.append("dependency cycle: " + " -> ".join(path + [nid]))
            return
        if nid in visited:
            return
        active.add(nid)
        for dep in sorted(adjacency.get(nid, [])):
            visit(dep, path + [nid])
        active.remove(nid)
        visited.add(nid)

    for nid in sorted(ix.nodes):
        visit(nid, [])
    return errors


# ---- commands ---------------------------------------------------------------

def cmd_ready(args):
    g = load()
    ix = Index(g)
    rows = []
    scope = None
    if getattr(args, "release", None):
        if args.release not in ix.nodes or ix.nodes[args.release]["type"] != "release":
            sys.exit("unknown release %r; expected one of %s" % (args.release, ", ".join(RELEASE_ORDER)))
        scope = set(ix.release_items(args.release))
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        unblocked = ix.milestone_unblocked(m["id"])
        if not unblocked and not args.all:
            continue
        for t in sorted(ix.milestone_work(m["id"]), key=lambda n: (n["type"] != "milestone_task", n["id"])):
            st = t.get("status", "NOT_STARTED")
            if st in ("COMPLETE",):
                continue
            if scope is not None and t["id"] not in scope:
                continue
            deps = ix.outs(t["id"], "after")
            deps_ok = all(ix.nodes[d].get("status") == "COMPLETE" for d in deps)
            if not deps_ok:
                continue
            rows.append((m["id"], t["id"], t["type"], st, "" if unblocked else "milestone blocked", t["title"]))
    for t in sorted(ix.by_type("dossier_task"), key=lambda n: n["id"]):
        if scope is not None:
            break
        if t.get("status", "NOT_STARTED") != "COMPLETE" and all(
                ix.nodes[d].get("status") == "COMPLETE" for d in ix.outs(t["id"], "after")):
            rows.append(("—", t["id"], t["type"], t.get("status", "NOT_STARTED"), "dossier only", t["title"]))
    if not rows:
        print("nothing ready (all upstream milestones incomplete or everything COMPLETE)")
        return
    print("%-4s %-14s %-15s %-13s %-18s %s" % ("MS", "ID", "TYPE", "STATUS", "NOTE", "TITLE"))
    for r in rows:
        print("%-4s %-14s %-15s %-13s %-18s %s" % r)


def cmd_show(args):
    g = load()
    ix = Index(g)
    n = ix.nodes.get(args.id)
    if not n:
        sys.exit("unknown node: %s" % args.id)
    print(json.dumps(n, indent=2, ensure_ascii=False))
    grouped = defaultdict(list)
    for e in ix.out.get(n["id"], []):
        grouped["→ " + e["type"]].append(e["to"])
    for e in ix.inc.get(n["id"], []):
        grouped["← " + e["type"]].append(e["from"])
    for k in sorted(grouped):
        vals = grouped[k]
        head = ", ".join(vals[:25])
        more = "" if len(vals) <= 25 else " … (+%d)" % (len(vals) - 25)
        print("%s (%d): %s%s" % (k, len(vals), head, more))
    m = ix.milestone_of(n["id"])
    if m:
        print("milestone: %s  state=%s  unblocked=%s" % (m, ix.milestone_state(m), ix.milestone_unblocked(m)))


def cmd_set(args):
    g = load()
    ix = Index(g)
    n = ix.nodes.get(args.id)
    if not n:
        sys.exit("unknown node: %s" % args.id)
    if n["type"] not in WORK_TYPES:
        sys.exit("status is only tracked on work items; %s is %s" % (n["id"], n["type"]))
    if args.state not in STATES:
        sys.exit("invalid state %r; allowed: %s" % (args.state, ", ".join(STATES)))
    n.setdefault("evidence", [])
    for ref in args.evidence or []:
        err = evidence_ref_error(ref)
        if err:
            sys.exit("cannot record evidence on %s: %s" % (n["id"], err))
        if ref not in n["evidence"]:
            n["evidence"].append(ref)
    if args.state in EVIDENCE_REQUIRED and not n["evidence"]:
        sys.exit("%s requires at least one --evidence reference (docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md)" % args.state)
    if args.state not in ("NOT_STARTED", "BLOCKED"):
        milestone = ix.milestone_of(n["id"])
        if milestone and not ix.milestone_unblocked(milestone):
            sys.exit("cannot %s %s: upstream milestones are incomplete" % (args.state, n["id"]))
        for d in ix.outs(n["id"], "after"):
            if ix.nodes[d].get("status") != "COMPLETE":
                sys.exit("cannot %s %s: prerequisite %s is %s" % (args.state, n["id"], d, ix.nodes[d].get("status")))
    prev = n.get("status", "NOT_STARTED")
    if args.state != prev:
        if args.state == "BLOCKED":
            if not args.note:
                sys.exit("BLOCKED requires --note recording the blocker, reproduction and next safe action (docs/93)")
            n["blocked_from"] = prev
        elif prev == "BLOCKED":
            ceiling = n.get("blocked_from", "NOT_STARTED")
            if LIFECYCLE.index(args.state) > LIFECYCLE.index(ceiling):
                sys.exit("cannot leave BLOCKED to %s: resume at or below the state it was blocked from (%s)" % (args.state, ceiling))
            n.pop("blocked_from", None)
        else:
            step = LIFECYCLE.index(args.state) - LIFECYCLE.index(prev)
            if step > 1:
                sys.exit("cannot %s %s from %s: the lifecycle moves one step at a time (%s); docs/93" % (
                    args.state, n["id"], prev, " -> ".join(LIFECYCLE)))
            if step < 0 and not args.note:
                sys.exit("moving %s back from %s to %s requires --note explaining the evidence expiry or regression (docs/93)" % (
                    n["id"], prev, args.state))
    n["status"] = args.state
    if args.note:
        n.setdefault("notes", []).append({"at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                                          "from": prev, "to": args.state, "note": args.note})
    if args.agent:
        n["owner_agent"] = args.agent
    n["status_changed_on"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    save(g)
    print("%s: %s → %s (evidence: %d)" % (n["id"], prev, args.state, len(n["evidence"])))


def cmd_attest(args):
    g = load()
    ix = Index(g)
    n = ix.nodes.get(args.id)
    if not n:
        sys.exit("unknown node: %s" % args.id)
    if n["type"] != "release_gate":
        sys.exit("attest applies to release gates only; %s is %s" % (n["id"], n["type"]))
    if not args.evidence:
        sys.exit("attest requires at least one --evidence reference with the gate's own proof (docs/61, docs/93)")
    pending = [t for t in ix.outs(n["id"], "requires_task") if ix.nodes[t].get("status") != "COMPLETE"]
    if pending:
        sys.exit("cannot attest %s: required tasks not COMPLETE: %s" % (n["id"], ", ".join(pending)))
    n.setdefault("evidence", [])
    for ref in args.evidence:
        err = evidence_ref_error(ref)
        if err:
            sys.exit("cannot record evidence on %s: %s" % (n["id"], err))
        if ref not in n["evidence"]:
            n["evidence"].append(ref)
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    n["attested_on"] = now
    if args.agent:
        n["attested_by"] = args.agent
    if args.note:
        n.setdefault("notes", []).append({"at": now, "note": args.note})
    save(g)
    print("%s: %s (evidence: %d)" % (n["id"], ix.gate_state(n["id"]), len(n["evidence"])))


def cmd_gates(args):
    ix = Index(load())
    print("%-12s %-15s %9s %8s  %s" % ("GATE", "STATE", "TASKS", "EVIDENCE", "TITLE"))
    for gate in sorted(ix.by_type("release_gate"), key=lambda n: n["id"]):
        req = ix.outs(gate["id"], "requires_task")
        done = sum(1 for t in req if ix.nodes[t].get("status") == "COMPLETE")
        print("%-12s %-15s %4d/%-4d %8d  %s" % (gate["id"], ix.gate_state(gate["id"]), done, len(req),
                                                len(gate.get("evidence", [])), gate["title"]))
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        gates = ix.milestone_gates(m["id"])
        if gates:
            print("%s is gated by %s; roll-up state %s" % (m["id"], ", ".join(gates), ix.milestone_state(m["id"])))


def cmd_releases(args):
    ix = Index(load())
    print("%-13s %-10s %11s %8s %7s  %s" % ("RELEASE", "STATE", "ITEMS", "BLOCKED", "GATES", "TITLE"))
    for rid in RELEASE_ORDER:
        if rid not in ix.nodes:
            continue
        items = ix.release_items(rid)
        statuses = [ix.nodes[i].get("status", "NOT_STARTED") for i in items]
        gates = ix.outs(rid, "requires_gate")
        sat = sum(1 for g in gates if ix.gate_state(g) == "SATISFIED")
        print("%-13s %-10s %5d/%-5d %8d %3d/%-3d  %s" % (rid, ix.release_state(rid), statuses.count("COMPLETE"), len(items),
                                                      statuses.count("BLOCKED"), sat, len(gates), ix.nodes[rid]["title"]))
    for rid in RELEASE_ORDER:
        if rid not in ix.nodes:
            continue
        startable = []
        for i in ix.release_items(rid):
            n = ix.nodes[i]
            if n.get("status", "NOT_STARTED") != "NOT_STARTED":
                continue
            m = ix.milestone_of(i)
            if m and not ix.milestone_unblocked(m):
                continue
            if all(ix.nodes[d].get("status") == "COMPLETE" for d in ix.outs(i, "after")):
                startable.append(i)
        print("%s: %d startable now%s" % (rid, len(startable), (": " + ", ".join(sorted(startable)[:6]) + (" …" if len(startable) > 6 else "")) if startable else ""))


def rollup(ix):
    out = []
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        work = ix.milestone_work(m["id"])
        c = Counter(w.get("status", "NOT_STARTED") for w in work)
        mt = sum(1 for w in work if w["type"] == "milestone_task")
        it = sum(1 for w in work if w["type"] == "imp_task")
        out.append({"id": m["id"], "title": m["title"], "state": ix.milestone_state(m["id"]),
                    "unblocked": ix.milestone_unblocked(m["id"]), "milestone_tasks": mt, "imp_tasks": it,
                    "complete": c.get("COMPLETE", 0), "blocked": c.get("BLOCKED", 0), "total": len(work),
                    "depends_on": ix.outs(m["id"], "depends_on")})
    return out


def cmd_status(args):
    ix = Index(load())
    print("%-4s %-12s %-9s %5s %5s %5s %5s  %-14s %s" % ("MS", "STATE", "UNBLOCKED", "MTASK", "IMP", "DONE", "BLKD", "DEPENDS", "TITLE"))
    for r in rollup(ix):
        print("%-4s %-12s %-9s %5d %5d %5d %5d  %-14s %s" % (
            r["id"], r["state"], "yes" if r["unblocked"] else "no", r["milestone_tasks"], r["imp_tasks"],
            r["complete"], r["blocked"], ",".join(r["depends_on"]) or "-", r["title"]))


def cmd_path(args):
    ix = Index(load())
    order = topo_milestones(ix)
    print("milestone order (topological): " + " → ".join(order))
    print("critical path (reliability spine): " + " → ".join(ix.g.get("critical_path", [])))


def topo_milestones(ix):
    ms = {m["id"]: set(ix.outs(m["id"], "depends_on")) for m in ix.by_type("milestone")}
    done, order = set(), []
    while len(order) < len(ms):
        ready = sorted(m for m in ms if m not in done and ms[m] <= done)
        if not ready:
            sys.exit("cycle in milestone dependencies")
        order.extend(ready)
        done.update(ready)
    return order


def cmd_stats(args):
    g = load()
    print("nodes by type:")
    for t, c in sorted(Counter(n["type"] for n in g["nodes"]).items()):
        print("  %-16s %d" % (t, c))
    print("edges by type:")
    for t, c in sorted(Counter(e["type"] for e in g["edges"]).items()):
        print("  %-16s %d" % (t, c))
    print("work items by status:")
    for t, c in sorted(Counter(n.get("status", "NOT_STARTED") for n in g["nodes"] if n["type"] in WORK_TYPES).items()):
        print("  %-16s %d" % (t, c))


STATE_STYLE = {
    "NOT_STARTED": "fill:#f3f4f6,stroke:#9ca3af,color:#111827",
    "IN_PROGRESS": "fill:#fef3c7,stroke:#d97706,color:#111827",
    "BLOCKED": "fill:#fee2e2,stroke:#dc2626,color:#111827",
    "COMPLETE": "fill:#dcfce7,stroke:#16a34a,color:#111827",
}


def render(g):
    ix = Index(g)
    roll = rollup(ix)
    L = []
    L.append("# Modbit Project Graph")
    L.append("")
    L.append("> Generated from `graph/project-graph.json` by `tools/graph.py render --write`. Do not edit by hand; "
             "edit the graph through `tools/graph.py set` or regenerate structure with `tools/build_graph.py`.  ")
    L.append("> Graph generated on %s; view rendered on %s." % (g.get("generated_on", "?"), g.get("updated_on", g.get("generated_on", "?"))[:10]))
    L.append("")
    L.append("## What the graph is")
    L.append("")
    L.append("One JSON file that answers *what exists, what depends on what, what proves what, and what state each work item is in*. "
             "Node and edge types:")
    L.append("")
    L.append("| Node type | Count | Meaning |")
    L.append("|---|---:|---|")
    cnt = Counter(n["type"] for n in g["nodes"])
    for t, desc in g["node_types"].items():
        L.append("| `%s` | %d | %s |" % (t, cnt.get(t, 0), desc))
    L.append("")
    L.append("| Edge type | Count | Meaning |")
    L.append("|---|---:|---|")
    ecnt = Counter(e["type"] for e in g["edges"])
    for t, desc in g["edge_types"].items():
        L.append("| `%s` | %d | %s |" % (t, ecnt.get(t, 0), desc))
    L.append("")
    L.append("## Milestone dependency graph (live status)")
    L.append("")
    L.append("```mermaid")
    L.append("flowchart LR")
    for r in roll:
        label = "%s<br/>%s<br/>%d/%d done" % (r["id"], r["title"].replace('"', "'"), r["complete"], r["total"])
        L.append('  %s["%s"]' % (r["id"], label))
    for r in roll:
        for d in r["depends_on"]:
            L.append("  %s --> %s" % (d, r["id"]))
    for r in roll:
        L.append("  style %s %s" % (r["id"], STATE_STYLE.get(r["state"], STATE_STYLE["NOT_STARTED"])))
    L.append("```")
    L.append("")
    L.append("Critical path (reliability spine): **" + " → ".join(g.get("critical_path", [])) + "**. "
             "Do not start broad multi-agent or cloud work before the single-agent durable local loop is E2E proven.")
    L.append("")
    L.append("## Milestone roll-up")
    L.append("")
    L.append("| Milestone | State | Unblocked | Milestone tasks | Implementation tasks | Complete | Blocked | Depends on | Proof |")
    L.append("|---|---|---|---:|---:|---:|---:|---|---|")
    for r in roll:
        proof = ix.nodes[r["id"]].get("proof", "")
        L.append("| %s %s | %s | %s | %d | %d | %d | %d | %s | %s |" % (
            r["id"], r["title"], r["state"], "yes" if r["unblocked"] else "no", r["milestone_tasks"], r["imp_tasks"],
            r["complete"], r["blocked"], ", ".join(r["depends_on"]) or "—", proof.replace("|", "/")))
    L.append("")
    L.append("## Subsystems → milestones")
    L.append("")
    L.append("Each canonical subsystem is a single-owner boundary (`docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`). "
             "The milestone shown is where the bulk of its `IMP-EV-*` tasks are scheduled; individual tasks may be scheduled elsewhere.")
    L.append("")
    L.append("```mermaid")
    L.append("flowchart TB")
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        subs = [s for s in ix.by_type("subsystem") if m["id"] in ix.outs(s["id"], "delivered_in")]
        if not subs:
            continue
        L.append('  subgraph %s["%s — %s"]' % (m["id"], m["id"], m["title"].replace('"', "'")))
        for s in subs:
            n_imp = len(ix.ins(s["id"], "owned_by"))
            L.append('    %s["%s<br/>%d tasks"]' % (s["id"].replace("-", "_"), s["title"].replace('"', "'"), n_imp))
        L.append("  end")
    L.append("```")
    L.append("")
    L.append("| Subsystem | Primary milestone | Crates / apps | Spec docs | REQ rows | IMP tasks | Decisions |")
    L.append("|---|---|---|---|---:|---:|---|")
    for s in sorted(ix.by_type("subsystem"), key=lambda n: n["id"]):
        L.append("| `%s` %s | %s | %s | %s | %d | %d | %s |" % (
            s["id"], s["title"], ", ".join(ix.outs(s["id"], "delivered_in")) or "—",
            ", ".join("`%s`" % c for c in s.get("crates", [])) or "—",
            ", ".join("`%s`" % d.split("_")[0] for d in ix.outs(s["id"], "specified_by")),
            len(ix.ins(s["id"], "owned_by_req")), len(ix.ins(s["id"], "owned_by")),
            ", ".join(ix.ins(s["id"], "constrains")) or "—"))
    L.append("")
    L.append("## Requirement → task → test chain")
    L.append("")
    L.append("```mermaid")
    L.append("flowchart LR")
    L.append('  REQ["REQ-EV-nnnn<br/>291 rows<br/>docs/40"] -->|implemented_by| IMP["IMP-EV-nnnn<br/>265 tasks<br/>docs/41"]')
    L.append('  REQ -->|qualified_by| QUAL["QUAL-EV-nnnn<br/>291 tests<br/>docs/42"]')
    L.append('  IMP -->|proven_by| QUAL')
    L.append('  REQ -->|owned_by_req| SUB["subsystem<br/>single owner"]')
    L.append('  IMP -->|owned_by| SUB')
    L.append('  IMP -->|scheduled_in| MS["milestone"]')
    L.append('  E2E["E2E / WSK / MEDIA / FI scenarios"] -->|proves| MS')
    L.append('  SUB -->|delivered_in| MS')
    L.append('  SUB -->|specified_by| DOC["docs/*"]')
    L.append('  DEC["MOD-* and ADR-R-* decisions"] -->|constrains| SUB')
    L.append('  EREQ["REQ-EPR<br/>20 additive rows / docs 49"] -->|implemented_by| ETASK["EPR-000..019"]')
    L.append('  ETASK -->|proven_by| EQUAL["QUAL-EPR / real and fault scenarios / docs 61"]')
    L.append('  ETASK -->|owned_by| SUB')
    L.append("```")
    L.append("")
    L.append("Disposition counts: " + ", ".join(
        "%s %d" % (k, v) for k, v in sorted(Counter(n["disposition"] for n in ix.by_type("requirement")).items())) + ".")
    L.append("")
    L.append("## Execution policy development dependencies")
    L.append("")
    L.append("EPR requirements extend the preserved EV ledger under DR-EPR-2026-09-05-v1.1; v1.0 conflicting clauses are superseded in place. All nodes use existing canonical owners. Gate nodes describe evidence required for activation; document existence is not proof.")
    L.append("")
    L.append("```mermaid")
    L.append("flowchart LR")
    epr_tasks = sorted([n for n in ix.by_type("imp_task") if n["id"].startswith("EPR-")], key=lambda n: n["id"])
    for t in epr_tasks:
        L.append('  %s["%s<br/>%s<br/>%s"]' % (t["id"].replace("-", "_"), t["id"], t["title"], t.get("status", "NOT_STARTED")))
    for t in epr_tasks:
        for dep in ix.outs(t["id"], "after"):
            L.append("  %s --> %s" % (dep.replace("-", "_").replace(".", "_"), t["id"].replace("-", "_")))
    if epr_tasks:
        L.append("  EPR_013 --> M10_3")
        L.append("  EPR_019 --> M10_3")
    L.append("```")
    L.append("")
    L.append("| Task | Milestone / phase | Status | Owner | Prerequisites | Requirement / qualification |")
    L.append("|---|---|---|---|---|---|")
    for t in epr_tasks:
        L.append("| %s | %s / %s | %s | %s | %s | %s / %s |" % (t["id"], t["milestone"], t["phase"], t.get("status", "NOT_STARTED"), t["subsystem"], ", ".join(ix.outs(t["id"], "after")), t["requirement"], t["qual"]))
    L.append("")
    L.append("| Activation gate | State | Required tasks | Attestation evidence | Acceptance |")
    L.append("|---|---|---|---|---|")
    for gate in sorted(ix.by_type("release_gate"), key=lambda n: n["id"]):
        L.append("| %s: %s | %s | %s | %s | %s |" % (gate["id"], gate["title"], ix.gate_state(gate["id"]), ", ".join(ix.outs(gate["id"], "requires_task")),
                                                   ", ".join(gate.get("evidence", [])) or "none", gate["acceptance"].replace("|", "/")))
    L.append("")
    L.append("Gate state is derived (docs/93): OPEN until every required task is COMPLETE, TASKS_COMPLETE, then SATISFIED once attested with `graph.py attest`. "
             "A milestone linked by `gated_by` rolls up GATED, not COMPLETE, until all its gates are SATISFIED: " +
             "; ".join("%s → %s" % (m["id"], ", ".join(ix.milestone_gates(m["id"]))) for m in ix.by_type("milestone") if ix.milestone_gates(m["id"])) + ".")
    L.append("")
    L.append("## Release readiness (derived)")
    L.append("")
    L.append("Releases are projections over work items and gates (docs/75). Readiness is computed, never set: NOT_READY until every included item is COMPLETE and every required gate SATISFIED; BLOCKED if any included item is BLOCKED.")
    L.append("")
    L.append("| Release | State | Included work items | Complete | Blocked | Required gates | Rule |")
    L.append("|---|---|---:|---:|---:|---|---|")
    for rid in RELEASE_ORDER:
        if rid in ix.nodes:
            items = ix.release_items(rid)
            st = [ix.nodes[i].get("status", "NOT_STARTED") for i in items]
            L.append("| %s: %s | %s | %d | %d | %d | %s | %s |" % (rid, ix.nodes[rid]["title"], ix.release_state(rid), len(items), st.count("COMPLETE"), st.count("BLOCKED"),
                                                            ", ".join(ix.outs(rid, "requires_gate")) or "none", ix.nodes[rid].get("rule", "").replace("|", "/")))
    L.append("")
    L.append("## Scoped v1.1 supersessions and source provenance")
    L.append("")
    L.append("```mermaid")
    L.append("flowchart LR")
    for edge in g["edges"]:
        if edge["type"] == "supersedes":
            L.append('  %s["%s"] -->|scoped supersession| %s["%s"]' % (edge["from"].replace("-", "_"), edge["from"], edge["to"].replace("-", "_"), edge["to"]))
    L.append("```")
    L.append("")
    L.append("| New decision | Earlier decision | Replaced/refined scope (other clauses survive) |")
    L.append("|---|---|---|")
    for edge in g["edges"]:
        if edge["type"] == "supersedes":
            L.append("| %s | %s | %s |" % (edge["from"], edge["to"], edge["scope"]))
    L.append("")
    for patch in ix.by_type("source_patch"):
        L.append("- `%s`: `%s`; SHA-256 `%s`." % (patch["id"], patch["path"], patch["sha256"]))
    L.append("")
    L.append("## Dossier work (excluded from product roll-ups)")
    L.append("")
    for t in ix.by_type("dossier_task"):
        L.append("- `%s`: %s; %s; evidence: %s" % (t["id"], t.get("status", "NOT_STARTED"), t["title"], ", ".join(t.get("evidence", [])) or "none"))
    L.append("")
    L.append("## Milestone tasks in execution order")
    L.append("")
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        tasks = sorted([ix.nodes[t] for t in ix.ins(m["id"], "part_of")], key=lambda n: n["order"])
        L.append("### %s — %s" % (m["id"], m["title"]))
        L.append("")
        L.append("| Task | Status | Title | Acceptance / note |")
        L.append("|---|---|---|---|")
        for t in tasks:
            L.append("| `%s` | %s | %s | %s |" % (t["id"], t.get("status", "NOT_STARTED"), t["title"].replace("|", "/"),
                                                (t.get("acceptance") or t.get("note") or "").replace("|", "/")))
        L.append("")
    L.append("## Proof scenarios by milestone")
    L.append("")
    L.append("| Milestone | Scenarios |")
    L.append("|---|---|")
    for m in sorted(ix.by_type("milestone"), key=lambda n: n["order"]):
        sc = sorted(ix.ins(m["id"], "proves"))
        L.append("| %s | %s |" % (m["id"], ", ".join(sc) or "—"))
    L.append("")
    L.append("## Document map")
    L.append("")
    L.append("| Section | Documents |")
    L.append("|---|---|")
    for sec in sorted(ix.by_type("section"), key=lambda n: n["order"]):
        docs = sorted(ix.ins(sec["id"], "in_section"))
        L.append("| %s | %s |" % (sec["title"], ", ".join("`%s`" % d.split("_")[0] for d in docs)))
    L.append("")
    L.append("## Query cookbook")
    L.append("")
    L.append("```bash")
    L.append("python3 tools/graph.py ready              # what can be started now (respects milestone + task ordering)")
    L.append("python3 tools/graph.py ready --all        # include tasks in blocked milestones")
    L.append("python3 tools/graph.py show IMP-EV-0013   # a task with its REQ, QUAL, subsystem, milestone, docs")
    L.append("python3 tools/graph.py show EPR-006       # EPR task, prerequisites, qualification and real/fault proofs")
    L.append("python3 tools/graph.py show core-runtime  # a subsystem with everything it owns")
    L.append("python3 tools/graph.py set M1.1 AUDITING --agent agent-7")
    L.append("python3 tools/graph.py set M1.1 COMPLETE --evidence run:2026-09-20/e2e-003 --evidence commit:deadbeef")
    L.append("python3 tools/graph.py status             # milestone roll-up for docs/98_BUILD_MANIFEST.md")
    L.append("python3 tools/graph.py path               # topological milestone order and critical path")
    L.append("python3 tools/graph.py render --write     # refresh this file")
    L.append("python3 tools/check_dossier.py            # integrity gate (run before every handoff)")
    L.append("```")
    L.append("")
    L.append("With `jq`:")
    L.append("")
    L.append("```bash")
    L.append("jq '.nodes[] | select(.type==\"imp_task\" and .status!=\"NOT_STARTED\") | {id,status,owner_agent}' graph/project-graph.json")
    L.append("jq -r '.edges[] | select(.type==\"owned_by\" and .to==\"browser\") | .from' graph/project-graph.json")
    L.append("```")
    L.append("")
    return "\n".join(L)


def cmd_render(args):
    g = load()
    text = render(g)
    if args.write:
        with open(VIEW_PATH, "w", encoding="utf-8") as fh:
            fh.write(text)
        print("wrote %s" % os.path.relpath(VIEW_PATH, ROOT))
    else:
        sys.stdout.write(text)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = p.add_subparsers(dest="cmd")
    a = sp.add_parser("ready"); a.add_argument("--all", action="store_true"); a.add_argument("--release"); a.set_defaults(fn=cmd_ready)
    a = sp.add_parser("show"); a.add_argument("id"); a.set_defaults(fn=cmd_show)
    a = sp.add_parser("set"); a.add_argument("id"); a.add_argument("state")
    a.add_argument("--evidence", action="append"); a.add_argument("--note"); a.add_argument("--agent"); a.set_defaults(fn=cmd_set)
    a = sp.add_parser("status"); a.set_defaults(fn=cmd_status)
    a = sp.add_parser("attest"); a.add_argument("id"); a.add_argument("--evidence", action="append"); a.add_argument("--note"); a.add_argument("--agent"); a.set_defaults(fn=cmd_attest)
    a = sp.add_parser("gates"); a.set_defaults(fn=cmd_gates)
    a = sp.add_parser("releases"); a.set_defaults(fn=cmd_releases)
    a = sp.add_parser("render"); a.add_argument("--write", action="store_true"); a.set_defaults(fn=cmd_render)
    a = sp.add_parser("path"); a.set_defaults(fn=cmd_path)
    a = sp.add_parser("stats"); a.set_defaults(fn=cmd_stats)
    args = p.parse_args(argv)
    if not args.cmd:
        p.print_help()
        return 1
    args.fn(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
