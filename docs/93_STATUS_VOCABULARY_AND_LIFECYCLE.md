# Status Vocabulary and Lifecycle Reconciliation

> **Authority date:** 2026-09-05  
> **Purpose:** the dossier previously carried three overlapping status ladders. This file is the single normative mapping. Where another file disagrees, this file wins and the other file must be corrected in the same change.

## Three different things carry status

| Thing | Ladder | Recorded in |
|---|---|---|
| **Decision** (an architecture/product choice) | `LOCKED`, `PROVISIONAL`, `EXPERIMENT`, `DEFERRED`, `REJECTED` (exactly one value per decision; compound labels are rejected by `tools/build_graph.py` and `tools/check_dossier.py`) | `02_AUTHORITY_AND_DECISIONS.md`, `72_RISK_REGISTER_AND_OPEN_DECISIONS.md` |
| **Requirement row** (`REQ-EV-*`) disposition | `ADOPT`, `ADAPT`, `EXPERIMENT`, `ALREADY COVERED`, `DEFERRED`, `REJECT` | `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` |
| **Work item** (task card, `IMP-EV-*`, milestone task `Mx.y`, milestone `Mx`) | agent task lifecycle below | task cards, `98_BUILD_MANIFEST.md`, `../graph/project-graph.json` |
| **Feature/capability** (what a release can claim) | feature depth ladder below | `83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`, `82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md`, release evidence bundle |
| **Existing code** found during audit | `PRODUCTION-WORKING`, `IMPLEMENTED-PARTIAL`, `SCAFFOLDED`, `DOCUMENTED-ONLY`, `BROKEN-DRIFTED`, `NOT-FOUND` | audit note on the task card (`84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md`) |

## Agent task lifecycle (work items)

`NOT_STARTED → AUDITING → IMPLEMENTING → WIRED → REAL_TESTING → E2E_PROVEN → COMPLETE`, plus `BLOCKED` from any state.

| State | Entry condition | Evidence required to leave |
|---|---|---|
| `NOT_STARTED` | task exists in graph/manifest | assigned agent + task card created |
| `AUDITING` | agent owns task | audit note with existing-code classification and first missing link |
| `IMPLEMENTING` | audit note recorded | code reaches a real effector/storage boundary |
| `WIRED` | production caller reaches implementation through real registration/routing/policy | integration test through production routing passes |
| `REAL_TESTING` | integration passes | linked `QUAL-EV-*` passes on real substrate; fault case exercised |
| `E2E_PROVEN` | qualification passes | linked E2E scenario passes on packaged/production-equivalent build with evidence bundle |
| `COMPLETE` | E2E proven | manifest row points to evidence refs; PR evidence template filled; remaining work empty |
| `BLOCKED` | any | blocker recorded with reproduction and next safe action |

Only `COMPLETE` means done. A milestone is `COMPLETE` only when every task in it is `COMPLETE`, the milestone proof in `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` has evidence, and every release gate linked to it by `gated_by` is `SATISFIED`.

## Transition rules (enforced by `tools/graph.py set`)

- Forward moves advance exactly one state along the lifecycle. A node cannot jump from `NOT_STARTED` to `COMPLETE`; dossier tasks walk the same ladder and their stage-applicability table explains each step.
- `E2E_PROVEN` and `COMPLETE` require evidence in the grammar below. Any state other than `NOT_STARTED` or `BLOCKED` requires upstream milestones and `after` prerequisites to be `COMPLETE`.
- `BLOCKED` requires `--note` recording the blocker, reproduction and next safe action. The state it was blocked from is stored, and the node may leave `BLOCKED` only to that state or an earlier one.
- Backward moves (evidence expiry, regression) may target any earlier state but require `--note`.
- Setting the current state again is allowed and only appends evidence or notes.

## Evidence tiers and release projections

The lifecycle is the same for every change. What differs by **behavioral risk** is the evidence that lets a change leave `REAL_TESTING` and `E2E_PROVEN`: release-critical changes need the real-effect and packaged E2E proof; iteration-tier changes, which by definition touch no effect-bearing behavior, canonical persistence, permissions or policy, execution, recovery, protocol or schema, security boundary or evidence semantics, need the packaged UI smoke suite against a real Core (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`). Mixed or uncertain changes are release-critical. The tier is recorded on the task card and in evidence references, never as a status.

Releases `ALPHA`, `BETA` and `RELEASE_ZERO` (`75_PHASED_RELEASE_PLAN_AND_READINESS.md`) are derived projections: `NOT_READY`, `BLOCKED` or `READY`, computed from included work items and required gates by `tools/graph.py releases`. They cannot be set, and they are not a fourth ladder.

## Release gate state (derived)

Release gates (`EPR-GATE-A..G`, `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md`) carry no lifecycle status. Their state is derived: `OPEN` while any required task is not `COMPLETE`; `TASKS_COMPLETE` when every required task is `COMPLETE`; `SATISFIED` when, in addition, a release agent has recorded the gate's own evidence (approved threshold profile, holdout, shadow, canary and rollback results) with `python3 tools/graph.py attest EPR-GATE-x --evidence ...`. Attestation is refused while any required task is incomplete. A milestone linked to gates by `gated_by` (M10 to all seven) rolls up `GATED`, not `COMPLETE`, until every gate is `SATISFIED`. `tools/check_dossier.py` G7 rejects malformed gate evidence and attestations recorded ahead of their tasks; `python3 tools/graph.py gates` prints the table.

## Feature depth ladder (capabilities)

`DECLARED → IMPLEMENTED → WIRED → E2E_PROVEN → COMPLETE`

This ladder describes the **state of the feature**, not the agent's activity. It is what `tools/evidence-check` and the release gate evaluate.

## Mapping between the two ladders

| Agent task state | Feature depth state it can prove at most |
|---|---|
| `NOT_STARTED`, `AUDITING` | `DECLARED` |
| `IMPLEMENTING` | `IMPLEMENTED` |
| `WIRED`, `REAL_TESTING` | `WIRED` |
| `E2E_PROVEN` | `E2E_PROVEN` |
| `COMPLETE` | `COMPLETE` |

A task may never claim a feature depth higher than its own state allows. A feature at `COMPLETE` requires every applicable task that contributes to it to be `COMPLETE`.

## Evidence expiry

If architecture, dependency version, protocol major version or the linked qualification test changes materially, the task moves back to `REAL_TESTING` and the feature back to `WIRED` until evidence is regenerated (`87_HANDOFF_AND_MANIFEST_PROTOCOL.md`).

## Machine encoding

The project graph (`../graph/project-graph.json`) stores `status` on every work-item node using exactly the agent task lifecycle strings above. `tools/check_dossier.py` rejects any other value and rejects `COMPLETE` without at least one `evidence` reference on the node.

## Evidence reference grammar

Every `evidence` entry on a work-item node is one string `<kind>:<value>`. `kind` is exactly one of the kinds below; `value` is non-empty printable ASCII without whitespace. `tools/graph.py set --evidence` and `tools/check_dossier.py` (check G3) reject anything else, including free text such as `done`. An `artifact:` value that is a package-relative path must exist in the package.

| Kind | Meaning | Example |
|---|---|---|
| `run` | qualification / E2E / security / performance run identifier | `run:2026-09-14T10:22Z/qual-ev-0012` |
| `test` | unit / property / integration test identifier | `test:core-runtime::lease_fencing` |
| `commit` | Git revision of the code under proof | `commit:abc123` |
| `revision` | non-Git revision digest, for example a sealed source index | `revision:sha256:<hex>` |
| `build` | build digest of the packaged binary | `build:sha256:<hex>` |
| `env` | environment / dependency digest | `env:sha256:<hex>` |
| `artifact` | retained file in the package, or an artifact digest | `artifact:evidence/dossier-gov/validation.json` |
| `event` | canonical event identifier | `event:evt_01J8` |
| `effect` | effect receipt identifier | `effect:rcpt_01J8` |
| `checkpoint` | checkpoint identifier | `checkpoint:ckpt_01J8` |

These kinds map onto the evidence classes in `92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md`. A reference names where proof lives; it does not by itself make a task `COMPLETE`. Decision statuses in `02_AUTHORITY_AND_DECISIONS.md` are validated against the decision ladder above by the same tools (check D7).

## EPR encoding extension

ADR-R-039..056 are LOCKED decisions. Historical SUPERSEDED annotations in doc 03 are provenance dispositions, not work-item states. REQ-EPR-000..019 are additive ADOPT requirements under DR-EPR-2026-09-05-v1.1. EPR-000..019 use the same lifecycle/evidence/prerequisite rules as IMP-EV and count in their scheduled product milestones.

DOC-EPR-001, DOC-EPR-002 and DOC-GOV-001 are `dossier_task` nodes outside product milestone roll-ups. They follow the same lifecycle for the actual dossier filesystem pipeline; docs 94/95/96 explicitly mark product provider/runtime stages non-applicable. Completing either requires retained generation/integrity/negative-test evidence and does not complete any EPR or milestone node. Gate nodes describe activation criteria and do not inherit completion from document existence.
