# SKILLS.md — Governed Procedures for Modbit Build Agents

> Second-highest authority after `AGENTS.md`. Each skill is a named, repeatable procedure with a trigger, required inputs, steps, outputs and the dossier files that govern it. Agents invoke skills by name in task cards and handoffs (for example `skill: existing-code-audit`). A skill's steps may be tightened by a later Decision Record but never loosened by an agent.

Skills are listed in the order they normally run inside the mandatory execution loop from `AGENTS.md`.

---

## 1. `task-intake`

**Trigger:** an agent needs work, or is assigned a task ID.  
**Inputs:** `graph/project-graph.json`.  
**Steps:**
1. `python3 tools/graph.py ready` — list work items whose dependencies are `COMPLETE` and whose milestone is unblocked.
2. Choose one item. Prefer the critical path `M0 → M1 → M2 → M4` while it is not proven.
3. `python3 tools/graph.py show <id>` — print the node with its requirements, tests, subsystem, docs and dependencies.
4. `python3 tools/graph.py set <id> AUDITING` and create a task card from `docs/86_TASK_CARD_TEMPLATE.md`.
**Outputs:** task card with identity block filled; graph node in `AUDITING`.  
**Governed by:** `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `docs/88_PARALLEL_AGENT_COORDINATION_RULES.md`, `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md`.

## 2. `context-bundle`

**Trigger:** before reading code for a task.  
**Inputs:** task card; graph node.  
**Steps:** load only the minimum bundle: `AGENTS.md`, task card, `docs/02_AUTHORITY_AND_DECISIONS.md`, the subsystem spec(s) linked from the graph, direct dependency specs, the linked `REQ-EV-*` rows, linked `QUAL-EV-*`/E2E tests, current `docs/98_BUILD_MANIFEST.md` and latest handoff. Discover source files by exact/symbol search, never by dumping directories. Record the revision/hash of every file used for a decision.  
**Outputs:** "Required reading" section of the task card with revisions.  
**Governed by:** `docs/89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md`, `docs/01_START_HERE_FOR_BUILD_AGENTS.md`.

## 3. `existing-code-audit`

**Trigger:** any task touching a subsystem where code already exists (always, after M0).  
**Inputs:** requirement surface from the task card.  
**Steps:** for each observable behavior trace `caller → transport/command → canonical owner → policy → persistence → effector → result/event/evidence → user/model projection`; stop at the first missing or fake boundary; classify as PRODUCTION-WORKING / IMPLEMENTED-PARTIAL / SCAFFOLDED / DOCUMENTED-ONLY / BROKEN-DRIFTED / NOT-FOUND; search for duplicate generations; inspect tests for mock-only proof.  
**Outputs:** audit note on the task card (classification, entry point, owner, effector boundary, tests found, first missing link, duplicate/drift risks).  
**Governed by:** `docs/84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md`, `docs/80_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md`, `docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md`.

## 4. `requirement-mapping`

**Trigger:** after the audit, before planning.  
**Steps:** enumerate every `REQ-EV-*` and `MOD-*` decision the task must satisfy; for each, name the feature-depth layers that apply (`DOMAIN CONTRACT + OWNER + PRODUCTION WIRING + POLICY + PERSISTENCE + REAL EFFECTOR + FAILURE/RECOVERY + EVIDENCE + PROJECTION + REAL TEST`); identify the `QUAL-EV-*`/E2E tests that will prove completion **before** writing code.  
**Outputs:** "Invariants" and "Verification" sections of the task card.  
**Governed by:** `docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md`, `docs/48_FEATURE_DEPTH_CONTRACTS.md`, `docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md`.

## 5. `vertical-slice-plan`

**Trigger:** after requirement mapping.  
**Steps:** plan the smallest **complete vertical slice**, not the smallest diff: domain/API change, persistence/migration, policy/capabilities, service/effector, UI/model projection, failure/recovery, evidence/observability. Confirm the slice maps to exactly one canonical owner. If a guardrail would be violated, stop and run `adr-proposal`.  
**Outputs:** "Implementation slice" section of the task card; graph node → `IMPLEMENTING`.  
**Governed by:** `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md` (Phase B), `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`, `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`.

## 6. `implement-behind-canonical-interfaces`

**Trigger:** slice approved.  
**Steps:** implement behind canonical interfaces; keep migrations/versioning compatible; add idempotency, cancellation, timeout, fail-closed policy and evidence emission **with** the success path; no `TODO`/`unimplemented!`/hard-coded success/disabled auth in reachable production code; exact dependency names only where `docs/35_DEPENDENCY_AND_BINDING_DECISIONS.md` allows.  
**Outputs:** code reaching a real effector/storage boundary.  
**Governed by:** `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md` (Phase C), `docs/82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md`, `docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md`, `docs/23_SECURITY_POLICY_EFFECT_LEDGER.md`.

## 7. `wire-and-verify-locally`

**Trigger:** implementation compiles.  
**Steps:** run formatting/lint/type/unit/property tests; then integration tests that go through **real registration/routing/policy** (production caller reaches implementation). Set graph node → `WIRED` only when a production route reaches the code.  
**Outputs:** test commands and outcomes recorded on the task card.  
**Governed by:** `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md` (Phase D), `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`.

## 8. `real-effect-proof`

**Trigger:** integration passes.  
**Steps:** execute the linked `QUAL-EV-*` and E2E paths against the real production-equivalent boundary (real filesystem/Git/process/SQLite/Chromium/sandbox guest/provider); kill and restart real processes wherever durability is claimed; use actual Chromium for browser behavior and actual guest execution for cloud behavior. Set graph node → `REAL_TESTING`, then `E2E_PROVEN` when the E2E scenario passes on a packaged/production-equivalent build.  
**Outputs:** run IDs, build digest, environment digest, evidence refs.  
**Governed by:** `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md` (Phase E), `docs/42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md`, `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md`, `docs/56_TOOL_CAPABILITY_CONFORMANCE.md`, `docs/57_SKILL_EVOLUTION_REAL_TESTS.md`, `docs/58_MULTIMODAL_MEDIA_REAL_TESTS.md`.

## 9. `fault-injection`

**Trigger:** real-effect proof passes.  
**Steps:** run at least the task-relevant fault/negative case from the catalog (crash before/after commit, transport drop with unknown outcome, stale lease/epoch, hostile content, takeover race, sandbox loss, disk full); assert invariants: no data loss, no duplicate protected effect, no authority escalation, no silent corruption. "No crash" is not sufficient. Where the task touches policy, idempotency, fencing, freshness, tenancy, redaction or path protection, also run a mutation check proving the test fails when the control is broken.  
**Outputs:** fault cases exercised, with results.  
**Governed by:** `docs/54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md`, `docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md`, `docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md`.

## 10. `evidence-capture`

**Trigger:** all proof steps done.  
**Steps:** assemble the evidence record: build digest, git revision, environment digest, test IDs, run IDs, capability IDs, provider/sandbox/browser versions, event/effect/artifact refs, pass/fail, duration, artifact digests. Screenshots are supplemental only. Fill `docs/90_PR_CHANGE_EVIDENCE_TEMPLATE.md`; "Remaining work" must be empty for `COMPLETE`.  
**Outputs:** evidence bundle refs written in the `kind:value` grammar of `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md` (`run:`, `test:`, `commit:`, `revision:`, `build:`, `env:`, `artifact:`, `event:`, `effect:`, `checkpoint:`); PR evidence section.  
**Governed by:** `docs/82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md`, `docs/92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md`, `docs/91_FEATURE_COMPLETION_AUDIT.md`.

## 11. `graph-and-manifest-update`

**Trigger:** any status change; always before handoff.  
**Steps:**
```bash
python3 tools/graph.py set <id> <STATE> [--evidence <ref> ...]
python3 tools/graph.py status            # milestone roll-up
python3 tools/check_dossier.py           # must pass
```
`--evidence` values must follow the docs/93 grammar and `artifact:` paths must exist in the package; the tool rejects free text. Status moves one step at a time; `BLOCKED` and backward moves require `--note`. Update the milestone row in `docs/98_BUILD_MANIFEST.md` only from the roll-up. Never bulk-mark a milestone because its directories exist. If evidence expired (architecture/dependency/protocol change), move the node back to `REAL_TESTING`.  
**Outputs:** consistent graph + manifest.  
**Governed by:** `docs/87_HANDOFF_AND_MANIFEST_PROTOCOL.md`, `docs/98_BUILD_MANIFEST.md`, `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md`.

## 12. `handoff`

**Trigger:** an agent stops work for any reason.  
**Steps:** write a machine-actionable handoff with: task and requirement IDs; branch/worktree and exact commit; lifecycle status; files added/changed/deleted; migrations/schema versions; interfaces/events changed; tests run and exact outcomes; real-system evidence refs; faults/security cases exercised; unresolved failures with reproduction commands; decisions/ADR refs; remaining acceptance criteria; next safe action. Forbidden phrases: "mostly done", "should work", "tests look good", "just needs cleanup".  
**Governed by:** `docs/87_HANDOFF_AND_MANIFEST_PROTOCOL.md`.

---

## Situational skills

## `adr-proposal`

**Trigger:** a task appears impossible without violating a guardrail, changing a LOCKED decision, or adding a parallel subsystem.  
**Steps:** stop implementation; write a Decision Record containing trigger/evidence, current behavior, proposed replacement, migration, compatibility, security impact, test impact, rollback and the explicit user approval required; set the graph node `BLOCKED` with the ADR ref.  
**Governed by:** `docs/02_AUTHORITY_AND_DECISIONS.md` (change control), `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`, `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`.

## `parallel-work-admission`

**Trigger:** before spawning or joining parallel agents.  
**Steps:** confirm one primary writer per canonical subsystem; independent worktrees/branches; serialized ownership for shared migrations/event schemas/policy/protocol; verify upstream contracts are stable (no broad parallelism on unstable Core/event/policy foundation); treat peer claims as untrusted until backed by code path + evidence. Merge is a transaction: rebase, semantic conflict check, rerun affected integration/E2E, verify migration order and event compatibility, then update the graph.  
**Governed by:** `docs/88_PARALLEL_AGENT_COORDINATION_RULES.md`, `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md`.

## `donor-extraction`

**Trigger:** considering reuse of old-repository code.  
**Steps:** classify EXTRACT / REFACTOR / WRAP TEMPORARILY / REFERENCE ONLY / DROP; pass the extraction gate (license, no proprietary reference dependency, dependency direction, canonical IDs/events, security review, real integration test, no old-shell coupling); add a `DONOR.md` entry.  
**Governed by:** `docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md`, `docs/36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md`.

## `dependency-admission`

**Trigger:** adding or upgrading an external dependency.  
**Steps:** evaluate dependency → integration → fork → reimplement → build → reject; record owner, license, security/maintenance check, justification and exit plan; pin exactly; confine vendor names to `docs/35_DEPENDENCY_AND_BINDING_DECISIONS.md` and lockfiles.  
**Governed by:** `docs/36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md`, `docs/70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md`.

## `release-gate`

**Trigger:** preparing a release candidate or closing a milestone.  
**Steps:** run every mandatory E2E in `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md` on signed candidate binaries; run `docs/91_FEATURE_COMPLETION_AUDIT.md` per production requirement; verify no item in `docs/73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` is present; verify `python3 tools/check_dossier.py` passes; attest each EPR gate A–G with `python3 tools/graph.py attest EPR-GATE-x --evidence ...` once its tasks are `COMPLETE` and the doc 61 evidence exists, and confirm `python3 tools/graph.py gates` shows all seven `SATISFIED` before M10 can roll up `COMPLETE`; archive the evidence bundle. Release Zero (`docs/60_RELEASE_ZERO_EXPANDED_PROOF.md`) must pass before any "end-to-end works" claim.  
**Governed by:** `docs/70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md`, `docs/83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`, `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`.

## `dossier-maintenance`

**Trigger:** any edit under `docs/`, `graph/` or the root governing files.  
**Steps:**
```bash
python3 tools/build_manifest.py   # regenerates MANIFEST.md + manifest.json
python3 tools/build_graph.py      # regenerates graph structure, preserves statuses/evidence
python3 tools/build_manifest.py   # hash the final graph and view
python3 tools/check_dossier.py --manifest  # EV + EPR coverage, refs, dependencies, statuses, hashes
python3 tools/test_dossier.py     # copied-package integration and negative checks
```
Never renumber existing files; new files take the next free number in their section. Requirement rows, dispositions and owners are LOCKED and change only via `adr-proposal`. The tools pin the sealed surface: `tools/check_dossier.py` expects exactly 291/265/291 EV rows/tasks/tests and `tools/dossier_epr.py` exactly EPR-000..019, ten scoped supersession pairs and gates A–G with their critical coverage; adding an EPR triplet, ADR, gate or supersession requires an approved Decision Record and a coordinated change to those constants and `tools/test_dossier.py` in the same reseal. Dossier-only work is a `dossier_task` (`DOC-*`) with its own handoff doc and `evidence/<task>/` bundle (docs 94/95/96).  
**Governed by:** `docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md`, `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`, `docs/00_MASTER_INDEX.md`.

---

## Skill packaging note

These skills are procedures for **build agents working on this repository**. They are distinct from the product's runtime Skill Registry (`docs/26_SKILL_REGISTRY_AND_EVOLUTION.md`), which governs skills that the Modbit product loads for its own agents. When the product's skill package format is implemented, this catalog should be exported into that format without changing its content.

## EPR dossier-maintenance applicability

DR-EPR-2026-09-05 authorizes additive EPR coverage without changing the existing 291 EV rows. For a dossier-only request, use an independent `dossier_task` (DOC-EPR-001) with explicit stage applicability in doc 94, not a product task whose real-system proof does not exist. The task follows the same status/evidence rules, but does not contribute to product milestone roll-ups. EPR product work uses REQ-EPR/EPR/QUAL-EPR in docs 49/61 and the ordinary mandatory loop. Graph status changes refresh its human view; refresh manifest hashes afterward.

## EPR v1.1 maintenance

DR-EPR-2026-09-05-v1.1 in doc 06 authorizes surgical amendments to the EPR ledger and six distinct delta tasks EPR-014..019; preserve existing EV rows and owners. DOC-EPR-002 (doc 95) is the independent dossier follow-up to DOC-EPR-001. Current architecture/contracts/tasks/gates are docs 27/38/49/61. Preserve both immutable root source patches and historical evidence; reseal graph and both manifest forms after status changes. No dossier proof completes a product EPR task.

## Governance maintenance (DOC-GOV-001)

DR-GOV-2026-09-05 in `docs/96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md` authorizes the 2026-09-05 maintenance reseal: `AGENTS.md`, `README.md` and this file aligned with the sealed package and five-step reseal; the evidence-reference grammar in doc 93 enforced by `tools/graph.py` and `tools/check_dossier.py`; MOD-SKILL-001 normalized to the single status `EXPERIMENT`; pinned tool constants documented in doc 74. DOC-GOV-001 follows DOC-EPR-002, is excluded from product roll-ups, and proves nothing about the product. Subsequent dossier-only tasks (DOC-GOV-002 onward) append an entry to `docs/97_DOSSIER_MAINTENANCE_LOG.md` instead of taking a new file number, because governance numbers 80–97 are exhausted; DOC-GOV-002 aligned docs 12/14/16/21/33/44 with EPR v1.1 placement, the `review_isolated` profile and reviewer projection. DOC-GOV-003 added `gated_by` edges, derived gate states with `graph.py attest`/`gates`, the `GATED` roll-up and one-step lifecycle transitions.
