# Build Manifest

> This file is updated by implementation agents. Initial status is intentionally **NOT_STARTED**; documentation completeness is not product implementation.

| Milestone | Scope | Status | Required proof |
|---|---|---|---|
| M0 | repository/CI/protocol generation | COMPLETE | clean clone build + architecture lint |
| M1 | durable local shell/Core | COMPLETE | create task, kill/restart app+Core, exact recovery |
| M2 | real local coding loop | COMPLETE | M2.1–M2.10 and 68 backlog tasks proven on hosted CI (macOS/Linux/Windows): real repo edit/test/review, approvals, receipts, headless CLI; the live-model E2E proof waits for provider credentials per DR-M2-001; 43 tasks rescheduled by DR-M2-002 |
| M3 | context intelligence | BLOCKED | M3.1–M3.9 and the M3 backlog proven on hosted CI (macOS/Linux/Windows): the retrieval stack, Context Pack and compaction, language tiers, the EPR routing spine (EPR-000..005, 014..016) and the paired benchmarks; PX-020 (the competence baseline) is BLOCKED per DR-M3-003 because it has no offline half and waits for provider credentials, which holds the roll-up; the live-model halves of EPR-000/002/005, IMP-EV-0251/0253, PX-022 and PX-028 are deferred by DR-M3-002/DR-M3-003; IMP-EV-0284 moved to M7 by DR-M3-004 |
| M4 | durable recovery spine | COMPLETE | M4.1–M4.6 COMPLETE on hosted CI: Protocol State store, compaction epochs (E2E-006 shape), workspace checkpoints (E2E-007), kernel lease fencing, durable terminal broker with cursor metadata (E2E-008), kill-point suite (23 boundaries; E2E-005 in full). Backlog sealed: IMP-EV-0012/0013/0055, 0073/0245 (typed failure diagnostics), 0242 (durable truth vs live control), 0077/0122/0123 (fork with BranchCarryoverCapsule, rewind preview, hash-checked revert, session tree), EPR-008 (factual assurance policy and RealizedRisk), EPR-017 (Acceptance Gate), EPR-009 (routing epochs and cache economics; live-provider half deferred by DR-M3-002). Rolled up 2026-09-11 on hosted run 34637382928 |
| M5 | procedural runtime/skills | IN_PROGRESS | real tools through isolated composition + skill provenance; M5.1 projection COMPLETE (run 34661374839); M5.2 QuickJS isolate, M5.3 registry-routed tools.* bindings, M5.4 proc.exec/proc.wait COMPLETE (run 34679501944); M5.5 skills next |
| M6 | subagents/fleet | NOT_STARTED | durable isolated child execution/conflict proof |
| M7 | live browser/computer use | NOT_STARTED | actual Chromium, same-session takeover, hostile-page test |
| M8 | isolated cloud execution | NOT_STARTED | real guest, tenant isolation, loss/recovery |
| M9 | memory/effects/security | NOT_STARTED | promotion policy + receipt chain + attack suite |
| M10 | release hardening | NOT_STARTED | full Release Zero proof + package evidence + EPR gates A–G SATISFIED |

## Task manifest rule

Individual tasks are tracked using `86_TASK_CARD_TEMPLATE.md`. A task row must carry requirement IDs and evidence before COMPLETE. Never bulk-mark an entire milestone complete because its directory/modules exist.

## Task-level status source of truth

Task-level (`IMP-EV-*`, `Mx.y`) status is stored on the corresponding node in `../graph/project-graph.json` and must use the lifecycle strings in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`. This table is the milestone roll-up of that graph. Regenerate the roll-up and validate consistency with:

```bash
python3 tools/graph.py status
python3 tools/check_dossier.py
```

A milestone row here may not read `COMPLETE` while any task node in that milestone is not `COMPLETE`; `tools/check_dossier.py` enforces this. M10 reads `GATED` while its tasks are `COMPLETE` but a release gate is not yet attested (`python3 tools/graph.py gates`, `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`).

## Execution policy adoption and live scope

Dossier edition V3.3 adopts EPR v1.1 via DR-EPR-2026-09-05-v1.1. DOC-EPR-002 tracks this reseal separately; current handoff/evidence is in `95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md`. DOC-GOV-001 (`96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md`) records the later governance/tooling maintenance reseal; it changes no product status. Subsequent dossier-only tasks are logged in `97_DOSSIER_MAINTENANCE_LOG.md`. Release readiness (ALPHA, BETA, RELEASE_ZERO) is derived, never recorded here: `python3 tools/graph.py releases` (`75_PHASED_RELEASE_PLAN_AND_READINESS.md`). Both root patches and prior dossier evidence remain immutable provenance. All product milestone and EPR-000..019 implementation states remain NOT_STARTED at adoption; no runtime benchmark, provider, recovery or rollout proof is claimed.

EPR tasks join existing M2/M4/M5/M6/M9/M10 roll-ups according to doc 49. M10 proof additionally includes the applicable execution-policy gates in doc 61. Development starts at graph-ready M0.1; once the real local loop is proven, EPR-000 establishes baseline before new routing. Numerical quality/benefit and rollout thresholds must be approved and measured before activation. PX rows REQ-PX-032..040 (DR-PX-2026-09-05-006) join M2 and M3 roll-ups, and `IMP-EV-0107` is scheduled in M2 by `MILESTONE_OVERRIDES` in `tools/build_graph.py` because the M2 repair loop depends on it; no task status changed.
