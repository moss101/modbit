# Modbit — Build Dossier and Project Driver

Modbit is an **agent-first engineering workspace**: a desktop Work + Code application where a user delegates software tasks to AI agents, supervises a fleet of them, and reviews real diffs, test output, browser actions and effect receipts, without living inside an IDE. One canonical Rust Core owns state, orchestration, policy, context, tool execution, evidence and recovery, and the same Core runs locally and in isolated cloud MicroVMs.

This repository currently contains the **complete implementation specification** (the "dossier") and the **project graph that drives the build**. It contains no product code yet; every milestone starts at `NOT_STARTED`.

## Layout

```text
README.md                       this file
AGENTS.md                       operating contract for AI build agents (highest authority)
SKILLS.md                       governed procedures agents follow, step by step
MANIFEST.md / manifest.json     every file with SHA-256, size, section and old name
MODBIT-PATCH-*.md               two immutable source patches kept as provenance; the numbered docs are authority
graph/
  project-graph.json            machine-readable driver: milestones → tasks → subsystems →
                                requirements → tests → docs, with live status
  PROJECT_GRAPH.md              human view (mermaid) and query cookbook
tools/
  build_manifest.py             regenerate MANIFEST.md + manifest.json
  build_graph.py                regenerate graph structure from docs (keeps statuses)
  dossier_epr.py                parse the additive EPR requirement/task/qualification surface
  graph.py                      query/update the graph: ready, show, set, status, render
  check_dossier.py              integrity gate for docs + graph + manifest + evidence grammar
  test_dossier.py               copied-package positive and negative tests of the tooling
evidence/                       retained evidence of dossier-only tasks (baseline, validation, test logs)
docs/                           83 specification files, uniquely numbered by section
```

## Where to start

| You are… | Read |
|---|---|
| an AI build agent | `AGENTS.md`, then `SKILLS.md`, then `docs/01_START_HERE_FOR_BUILD_AGENTS.md` |
| a human architect | `docs/00_MASTER_INDEX.md`, `docs/02_AUTHORITY_AND_DECISIONS.md`, `docs/11_SYSTEM_ARCHITECTURE.md` |
| a reviewer checking progress | `graph/PROJECT_GRAPH.md`, `docs/98_BUILD_MANIFEST.md`, `python3 tools/graph.py status` |
| someone adding or editing a spec | `SKILLS.md` → `dossier-maintenance` |

## The numbering scheme

| Range | Section |
|---|---|
| 00–09 | Authority and orientation |
| 10–29 | Architecture and canonical subsystems |
| 30–39 | Implementation specifications |
| 40–49 | Requirements (291 `REQ-EV-*`), tasks (265 `IMP-EV-*`), qualifications (291 `QUAL-EV-*`) plus 20 additive REQ-EPR/EPR/QUAL-EPR, traceability |
| 50–69 | Verification and testing, including 25 E2E release-gate scenarios and Release Zero |
| 70–79 | Delivery and operations |
| 80–97 | Agent process and governance |
| 98 | Live build manifest |

Numbers are stable identifiers. Never renumber; add new files at the next free number in their section.

## The project graph

`graph/project-graph.json` is the single place where "what to do next" is answered. It links:

- 11 milestones (`M0`–`M10`) with dependency edges and the critical path `M0 → M1 → M2 → M4`;
- milestone tasks (`M1.3`, `M7.6`, …) in execution order;
- canonical subsystems (the single-owner systems that may never be duplicated) with their crates and spec files;
- every `REQ-EV-*` row, its `IMP-EV-*` task and `QUAL-EV-*` test, attached to a subsystem and therefore a milestone;
- E2E, skill-evolution, media and fault-injection scenarios attached to the milestone they prove;
- `MOD-*` and adopted `ADR-R-*` decisions attached to the subsystems they constrain;
- every document, its section and the documents it references.

Every work-item node carries a `status` from the lifecycle `NOT_STARTED → AUDITING → IMPLEMENTING → WIRED → REAL_TESTING → E2E_PROVEN → COMPLETE` (+ `BLOCKED`) and an `evidence` list. `COMPLETE` without evidence is rejected by the tooling.

```bash
python3 tools/graph.py ready              # what can be started now
python3 tools/graph.py show M1.5          # one node with all its links
python3 tools/graph.py set IMP-EV-0013 WIRED
python3 tools/graph.py set IMP-EV-0013 COMPLETE --evidence run:2026-09-20/qual-ev-0013 --evidence commit:abc123
python3 tools/graph.py status             # milestone roll-up (M10 reads GATED until gates A–G are attested)
python3 tools/graph.py gates              # release-gate readiness: OPEN / TASKS_COMPLETE / SATISFIED
python3 tools/graph.py releases           # ALPHA / BETA / RELEASE_ZERO readiness, derived from task state and gates
python3 tools/graph.py ready --release ALPHA   # what can be started now inside Alpha
python3 tools/graph.py attest EPR-GATE-A --evidence artifact:evidence/release/gate-a.json
python3 tools/graph.py render > graph/PROJECT_GRAPH.md   # refresh the mermaid view
python3 tools/check_dossier.py            # integrity gate
```

The tools need only Python 3.9+ and the standard library. Evidence references use the `kind:value` grammar defined in [docs/93](docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md); the tools reject anything else. Status moves one lifecycle step at a time, and release gates are attested only after their tasks are complete.

## Non-negotiables in one paragraph

Names do not satisfy behavior. A feature is complete only when its domain contract, canonical owner, production wiring, policy, persistence, real effector, failure/recovery, evidence, projection and a real test all exist. No mock closes a production feature. Every protected effect has a receipt. Memory is not recovery. There is exactly one scheduler, one policy kernel, one event store, one context engine, one tool runtime. Release Zero must pass on a packaged build before anyone says "Modbit works end to end".

## Status

Specification: V3.3 EPR v1.1 (2026-09-05): 291 preserved REQ-EV rows plus 20 additive REQ-EPR rows, EPR-000..019 work packages and QUAL-EPR proofs. Governance maintenance reseal DOC-GOV-001 (2026-09-05) added the evidence-reference grammar and decision-status validation; see [doc 96](docs/96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md). DOC-GOV-002 aligned the implementation specifications with EPR v1.1, and DOC-GOV-003 made release-gate attestation and one-step lifecycle transitions tool-enforced, and DOC-GOV-004 carried both patches into the PRD, desktop, cloud, durability, tool-inventory, conformance and definition-of-done docs with a verified source coverage map in doc 27; see the [maintenance log](docs/97_DOSSIER_MAINTENANCE_LOG.md). DR-PX ([doc 07](docs/07_PRODUCT_EXTENSION_DECISION_RECORD.md)) opened the additive product-extension ledger ([doc 62](docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md)) and the phased releases ([doc 75](docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md)); the requirement count in this README is the frozen base, and effective totals come from `python3 tools/check_dossier.py`.  
Implementation: `NOT_STARTED` on every milestone. See `docs/98_BUILD_MANIFEST.md`.

## Execution policy development

Start with [adoption and compatibility](docs/06_EPR_V1_1_SUPERSESSION_DECISION.md), [routing architecture](docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md), [contracts and algorithms](docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md), [tasks and dependencies](docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md) and [qualification/rollout gates](docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md). Both root patches are immutable provenance, with v1.1 superseding only its declared conflicts. One ConditionalExecutionPlan and prevalidated slots extend the canonical runtime; DIRECT/CASCADE/CRITIQUE are derived labels; product implementation remains NOT_STARTED.

`python3 tools/graph.py show EPR-006` displays a complete dependency/requirement/test neighborhood. DOC-EPR-002 tracks only this dossier refinement/reseal outside product roll-ups. Its audit and exact validation evidence are in [the handoff](docs/95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md).

After changing documents or statuses, run the regeneration sequence in [package integrity](docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md). Hash the final graph, run `python3 tools/check_dossier.py --manifest`, then `python3 tools/test_dossier.py` for copied-package negative checks. Do not mark a runtime task complete from these documentation checks.
