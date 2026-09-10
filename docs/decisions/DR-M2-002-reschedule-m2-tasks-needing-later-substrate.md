---
id: DR-M2-002
title: Reschedule forty-three M2-labelled tasks whose qualifications need M3–M10 substrate (three proven dependents move with their prerequisites)
status: accepted
date: 2026-09-10
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks" on 2026-09-09/10; scheduling decision under docs/43 and docs/82 (no simulated substrate is claimed as proof)
---

# DR-M2-002 — Reschedule M2 tasks that need later substrate

## Trigger / evidence

All ten M2 milestone tasks (M2.1–M2.10) and 72 of the M2-labelled
`IMP-EV-*` / `PX-*` tasks are COMPLETE on hosted CI (macOS, Linux, Windows).
The remaining 39 M2-labelled tasks have qualifications that require product
substrate M2 does not contain. Completing them in M2 would need a simulated
context engine, execution backend, browser, router, adapter suite or live
provider, which `docs/82` forbids.

| Task | Needs | New milestone |
|---|---|---|
| IMP-EV-0070 (diagnostic change window), IMP-EV-0132 (evidence search), IMP-EV-0134, IMP-EV-0177, IMP-EV-0229 (deferred tool search, lazy schema context, toolsets), IMP-EV-0188 (provider media split) | Context Engine, session index, context economy, multimodal provider path (M3) | M3 |
| PX-016, PX-018, PX-038, PX-039, PX-040 (change strategy, RepairAttempt records, scope policy, repair policy, harness contracts) | retrieval-before-edit records, evidence ledger and the Context Pack Compiler (M3.7); the M2 loop keeps plan gate, failure signatures, no-progress detection, budgets and scope counting | M3 |
| PX-019, PX-034, PX-036, PX-037 (self-review/completion, staged targeting, flaky protocol, diff invariants) | already proven on the M2.7/M2.8 substrate and sealed COMPLETE; they move with their docs/62 prerequisites PX-018, PX-033 and PX-016 so the dependency order stays acyclic | M3 |
| PX-026 (Alpha language baseline), PX-033 (normalized reports incl. real pytest) | exact/BM25 retrieval (M3.7) and the python-service fixture; cargo and vitest reports are proven by M2.8 | M3 |
| PX-022 (onboarding within five minutes), EPR-000 (direct baseline measurement) | a live provider test model (DR-M2-001 credentials) and the M3 benchmark harness | M3 |
| IMP-EV-0043 (ProtocolCapabilitySet), IMP-EV-0265 (Q&A → plan → visual review), PX-001 (thin-client conformance suite) | external adapters, fleet and attention UX (M6) | M6 |
| IMP-EV-0088 (semantic UI risk), IMP-EV-0147 (browser/E2E evidence) | live browser surface (M7) | M7 |
| IMP-EV-0021, IMP-EV-0062, IMP-EV-0063, IMP-EV-0146, IMP-EV-0176 (environment hierarchy, snapshots, handoff bundle, blueprints, workspace capsule) | isolated execution backend and local→cloud handoff (M8) | M8 |
| EPR-001..005, EPR-014..016 (router machinery) | follow the EPR-000 baseline evidence; must precede the M4/M5 EPR tasks that depend on them | M3 |
| IMP-EV-0040, IMP-EV-0041 (device policy, hot revalidation), IMP-EV-0029, IMP-EV-0030 (capability routing, specialist chains) | security hardening and the router after the EPR baseline (M9) | M9 |
| IMP-EV-0212 (docs/example runner) | release-hardening gate (M10) | M10 |

## Current behavior

The 39 tasks block the M2 milestone while being unimplementable without M3+
code; `tools/build_graph.py` overrides applied only to docs/41 tasks.

## Proposed replacement

1. `MILESTONE_OVERRIDES` in `tools/build_graph.py` carries the table above
   with this record as the reason; the override post-pass now applies to PX
   and EPR tasks too (their ledger rows in docs/62 and docs/49 are untouched;
   the graph node records `milestone_override`).
2. Every rescheduled task keeps its requirement, qualification and evidence
   rules; nothing is marked complete by this record.
3. M2 is complete when its milestone tasks and the M2-labelled tasks not
   listed above are COMPLETE; PX-017 and PX-032 are sealed in the
   same batch as this record.

## Migration

None: status is only ever written by `tools/graph.py`.

## Compatibility

No interface change.

## Security impact

None.

## Verification

`python3 tools/build_graph.py && python3 tools/test_dossier.py` accept the
overrides; `graph/PROJECT_GRAPH.md` lists each moved task under its new
milestone with the reason.
