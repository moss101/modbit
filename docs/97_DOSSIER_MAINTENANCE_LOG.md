# Dossier maintenance log

Append-only record of dossier-only maintenance tasks after DOC-GOV-001. Each entry is one `dossier_task` node with its own Decision Record, stage applicability, handoff and `evidence/<task>/` bundle, tracked outside product milestone roll-ups. Earlier entries are never edited; a correction is a new entry. Governance numbers 80–97 are exhausted, so later tasks append here instead of taking a new file number.

## DOC-GOV-002 — Align implementation specs with EPR v1.1

### Identity and authority

- Task: DOC-GOV-002 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-001 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-05-002, approved for dossier adoption. On 2026-09-05 the user asked which enhancements remained after DOC-GOV-001 and explicitly requested item 1 of that answer: bring the implementation specifications that build agents read for EPR-014, EPR-015 and EPR-018 into line with EPR v1.1, and place the dossier under version control at `https://github.com/moss101/modbit.git`.
- Scope: docs 12, 14, 16, 21, 33, 44 and the stale payload bullet in doc 74; this log; graph builder and tests for the new node; pointer updates in docs 00/01/98, `../AGENTS.md`, `../README.md`, `../SKILLS.md`. No requirement row, owner, ADR clause, EPR task, gate or product status changes.
- Revision before change: `../evidence/dossier-gov-002/baseline.json`, which records the `main` baseline commit and every package hash.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Review on 2026-09-05: doc 12 still placed a "deterministic execution plan compiler" under a v1.0 heading and named no location for the plan validator, the Outcome Statistics store or the reviewer environment; doc 33 and doc 44 said "prior/realized risk" and doc 14 "prior risk", which reads as the learned risk scalar v1.1 removed; doc 21 defined no review execution profile although EPR-018 requires a real sandbox boundary; doc 16 said nothing about reviewer tool projection although ADR-R-053 constrains it. Docs 16 and 21 were unchanged since before EPR v1.0. |
| Current behavior | Agents picking up EPR-014/015/018 at M2 would read v1.0 placement and have to infer v1.1 locations from docs 27/38/49. |
| Replacement | Doc 12: v1.1 placement table by component, owner, crate and delta task, plus crate-tree comments. Doc 33: startup pins `stats_version`; Verification Engine owns the Acceptance Gate; the wiring section describes compiler, admission, factual `RealizedRisk`, independent `AcceptanceGateResult`, precompiled slots and separate attribution. Doc 21: `review_isolated` execution profile realized by the existing Execution Router. Doc 16: per-leg projection with the reviewer projection and kernel denial. Docs 14/44: PolicyEnvelope wording without a learned risk score. Doc 74: payload bullet matches the package. |
| Migration | Documentation only. No graph live state other than DOC-GOV-002 changes; no evidence reference is rewritten. |
| Compatibility | Consistent with ADR-R-049..056, docs 06/27/38/49/61 and the canonical owner table; no owner moves. Graph schema 1.2 unchanged; one `dossier_task`, one `change_record` and their edges are added. |
| Security impact | None on the product. The reviewer boundary is now specified at the execution-profile and tool-projection layers as well as the policy layer, which narrows how EPR-018 can be implemented. |
| Test impact | One test added to `../tools/test_dossier.py` asserting the v1.1 placement wording is present in docs 12/16/21/33 and the superseded wording is absent; the governance test also checks DOC-GOV-002. |
| Rollback | Restore the files in the inventory below from `baseline.json` hashes, or revert the change commit on the branch, and rerun the reseal. Reversal requires another approved Decision Record because governing text is affected. |
| Explicit user approval | Given in chat on 2026-09-05: "implement item 1 and use github". |

### Stage applicability

| Stage | DOC-GOV-002 execution |
|---|---|
| SELECT / READ / TRACE / AUDIT / MAP / PLAN | Read docs 12/14/16/21/23/27/33/38/44/49; baseline hashes and Git anchor; first missing links listed above |
| IMPLEMENT VERTICAL SLICE | Spec text, this log entry, graph builder node, test, regenerated graph and manifests |
| TEST LOCALLY / INTEGRATION | Actual Python CLIs and copied-package positive/negative tests |
| TEST REAL EFFECT | Real filesystem regeneration and SHA-256 verification of both manifest forms; Git commit and push to the named remote |
| INJECT FAILURE | Every prior negative case in the copied-package suite; the new wording guard fails on a copy with superseded wording |
| Product provider/sandbox/SQLite/Git qualification | Non-applicable: no product source exists and no product claim is made |
| CAPTURE / UPDATE / HANDOFF | Evidence under `../evidence/dossier-gov-002/`; only DOC-GOV-002 status set via graph.py; graph and manifests regenerated; change and seal commits on the work branch |

### Status and handoff

- **Task and requirements:** DOC-GOV-002 under DR-GOV-2026-09-05-002. The 291 REQ-EV rows, docs 40/41/42/49/61, both root patches and every prior evidence bundle are byte-identical to the baseline.
- **Revision:** repository `https://github.com/moss101/modbit.git`, branch `dossier/doc-gov-002-impl-spec-v1-1-alignment` from `main` at the baseline commit in `baseline.json`. The exact content revision is `source_revision_sha256` in `../evidence/dossier-gov-002/validation.json` (same algorithm as DOC-EPR-002 and DOC-GOV-001). The change commit hash is recorded as `commit:` evidence on the DOC-GOV-002 graph node and in the pull request; the seal commit changes only graph live state and the manifests, so `source_revision_sha256` identifies these sources exactly.
- **Interfaces:** graph schema 1.2 unchanged; nodes DR-GOV-2026-09-05-002 and DOC-GOV-002 added with their edges; no tool behavior changed.
- **Evidence:** `../evidence/dossier-gov-002/baseline.json`, `../evidence/dossier-gov-002/tests.log`, `../evidence/dossier-gov-002/validation.json`. This is a SHA-256 integrity reseal with a Git anchor, not a product certification.
- **Checks:** Python syntax parse of all six tools; `python3 tools/test_dossier.py` (30 tests) all passed; `build_manifest`, `build_graph`, `build_manifest`, `check_dossier --manifest` all exit 0; exact commands, outputs and digests in validation.json. Graph and manifests are regenerated again after completion so status and hashes are current.
- **Faults exercised:** the full copied-package negative suite, plus the new wording guard.
- **Remaining dossier acceptance:** none after the final integrity check.
- **Remaining product work/blockers:** unchanged from docs 95/96. Production source is absent; all milestones and EPR-000..019 remain NOT_STARTED; numerical EPR thresholds still need approved measured profiles.
- **Next safe action:** merge the pull request after review, then `python3 tools/graph.py ready` and take M0.1.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `.gitignore` (baseline commit) |
| Added | `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Added | `evidence/dossier-gov-002/baseline.json` |
| Added | `evidence/dossier-gov-002/tests.log` |
| Added | `evidence/dossier-gov-002/validation.json` |
| Changed | `AGENTS.md` |
| Changed | `MANIFEST.md` |
| Changed | `README.md` |
| Changed | `SKILLS.md` |
| Changed | `docs/00_MASTER_INDEX.md` |
| Changed | `docs/01_START_HERE_FOR_BUILD_AGENTS.md` |
| Changed | `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md` |
| Changed | `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md` |
| Changed | `docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` |
| Changed | `docs/21_TERMINAL_EXECUTION_AND_SANDBOX.md` |
| Changed | `docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` |
| Changed | `docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md` |
| Changed | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| Changed | `docs/98_BUILD_MANIFEST.md` |
| Changed | `graph/PROJECT_GRAPH.md` |
| Changed | `graph/project-graph.json` |
| Changed | `manifest.json` |
| Changed | `tools/build_graph.py` |
| Changed | `tools/test_dossier.py` |

## DOC-GOV-003 — Enforce release-gate attestation and one-step lifecycle transitions

### Identity and authority

- Task: DOC-GOV-003 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-002 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-05-003, approved for dossier adoption. On 2026-09-05 the user answered "continue" to the remaining enhancement list; items 2 and 3 of that list are implemented here.
- Scope: `../tools/graph.py`, `../tools/build_graph.py`, `../tools/check_dossier.py`, `../tools/test_dossier.py`; docs 43, 61 (prose only), 74, 93, 98; pointers in doc 00, `../AGENTS.md`, `../README.md`, `../SKILLS.md`; this entry. No requirement row, owner, ADR clause, EPR task, gate definition or product status changes.
- Revision before change: `../evidence/dossier-gov-003/baseline.json`, recording the branch base commit and every package hash.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Docs 43 and 98 claimed M10 proof includes gates A–G, but no edge linked gates to M10 and the roll-up ignored them. `graph.py set` accepted a jump from `NOT_STARTED` to `COMPLETE` given evidence, contradicting the per-state entry conditions in doc 93; DOC-EPR-002, DOC-GOV-001 and DOC-GOV-002 were all sealed with such jumps. |
| Current behavior | Gate readiness was invisible and unenforced; lifecycle order was advisory. |
| Replacement | New edge `gated_by` (M10 to each gate). Gate state is derived, never stored: `OPEN`, `TASKS_COMPLETE`, `SATISFIED`; `graph.py attest` records gate evidence only after all required tasks are `COMPLETE`; `graph.py gates` prints readiness; a gated milestone rolls up `GATED` until all gates are `SATISFIED`; check G7 rejects malformed or premature attestations and G5 accepts `GATED`. `graph.py set` enforces one-step forward moves, `--note` for `BLOCKED` and backward moves, and resumption from `BLOCKED` at or below the stored `blocked_from` state. |
| Migration | Live state only for gates (empty `evidence` lists) and `blocked_from` preservation. Existing statuses are untouched; earlier dossier tasks keep their recorded transitions as history, and this task walks the full ladder. |
| Compatibility | Graph schema 1.2; one new edge type, seven new edges, two governance nodes. Existing commands unchanged except stricter `set`. |
| Security impact | None on the product. Governance becomes stricter: M10 cannot be declared complete without attested gate evidence, and completion cannot skip proof states. |
| Test impact | Two tests added: one-step transitions with `BLOCKED` and backward rules; attestation refusal, `GATED` roll-up, attestation to `COMPLETE`, and the G7 finding for a premature attestation. |
| Rollback | Revert the branch or restore the inventory below from `baseline.json` hashes and rerun the reseal. Requires another Decision Record because docs 43/61/93/98 are governing text. |
| Explicit user approval | Given in chat on 2026-09-05 ("continue") after the enhancement list naming these two items. |

### Stage applicability

| Stage | DOC-GOV-003 execution |
|---|---|
| AUDITING | Read graph.py/build_graph.py/check_dossier.py paths for status and gates; baseline hashes; first missing links above |
| IMPLEMENTING | Tool changes, doc text, this entry |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite including the two new tests |
| E2E_PROVEN | Change commit pushed; evidence bundle retained |
| COMPLETE | Status set through the one-step ladder with evidence; seal commit |
| Product provider/sandbox/SQLite/Git qualification | Non-applicable: no product source exists and no product claim is made |

### Status and handoff

- **Task and requirements:** DOC-GOV-003 under DR-GOV-2026-09-05-003. Docs 40/41/42/49, both root patches and every prior evidence bundle are byte-identical to the baseline; doc 61 changed in prose only and its gate/qualification/scenario tables parse identically.
- **Revision:** repository `https://github.com/moss101/modbit.git`, branch `dossier/doc-gov-003-gate-and-lifecycle-enforcement`, based on the DOC-GOV-002 branch at the baseline commit in `baseline.json`. Content revision: `source_revision_sha256` in `../evidence/dossier-gov-003/validation.json`. The change commit hash is recorded as `commit:` evidence on the DOC-GOV-003 node and in the pull request; the seal commit changes only graph live state and manifests.
- **Interfaces:** graph schema 1.2; edge type `gated_by`; release-gate nodes carry `evidence`, `attested_on`, `attested_by`; work items may carry `blocked_from`; commands `attest` and `gates`; roll-up value `GATED`.
- **Evidence:** `../evidence/dossier-gov-003/baseline.json`, `../evidence/dossier-gov-003/tests.log`, `../evidence/dossier-gov-003/validation.json`.
- **Checks:** Python syntax parse of all six tools; `python3 tools/test_dossier.py` (32 tests) all passed; `build_manifest`, `build_graph`, `build_manifest`, `check_dossier --manifest` all exit 0; exact outputs in validation.json.
- **Faults exercised:** illegal lifecycle jump, `BLOCKED` without note, resume above `blocked_from`, backward move without note, attestation before tasks complete, attestation without evidence, attestation on a non-gate, premature attestation flagged by G7, plus the full prior negative suite.
- **Remaining dossier acceptance:** none after the final integrity check.
- **Remaining product work/blockers:** unchanged. Numerical EPR thresholds still need approved measured profiles; they are now the evidence `attest` will carry.
- **Next safe action:** merge the DOC-GOV-002 and DOC-GOV-003 pull requests in order, then `python3 tools/graph.py ready` and take M0.1.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `evidence/dossier-gov-003/baseline.json` |
| Added | `evidence/dossier-gov-003/tests.log` |
| Added | `evidence/dossier-gov-003/validation.json` |
| Changed | `AGENTS.md` |
| Changed | `MANIFEST.md` |
| Changed | `README.md` |
| Changed | `SKILLS.md` |
| Changed | `docs/00_MASTER_INDEX.md` |
| Changed | `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` |
| Changed | `docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` |
| Changed | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| Changed | `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md` |
| Changed | `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `docs/98_BUILD_MANIFEST.md` |
| Changed | `graph/PROJECT_GRAPH.md` |
| Changed | `graph/project-graph.json` |
| Changed | `manifest.json` |
| Changed | `tools/build_graph.py` |
| Changed | `tools/check_dossier.py` |
| Changed | `tools/graph.py` |
| Changed | `tools/test_dossier.py` |

## DOC-GOV-004 — Integrate both EPR patches into product-facing docs and add the source coverage map

### Identity and authority

- Task: DOC-GOV-004 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-003 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-05-004, approved for dossier adoption. On 2026-09-05 the user restated the original task: complete both root patches into the docs, revise the architecture, and enhance the application overall, with everything consolidated on `main`.
- Scope: docs 10, 17, 19, 24, 32, 56, 83 (product, tool inventory, durability, cloud, desktop, conformance, definition of done); doc 27 §28 coverage map; check D8 and its test; pointers in docs 00/74, `../README.md`, `../SKILLS.md`; this entry. No requirement row, owner, ADR clause, EPR task, gate definition or product status changes.
- Revision before change: `../evidence/dossier-gov-004/baseline.json` (main commit and every package hash).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Coverage audit on 2026-09-05: every section of both patches was carried by some doc, but no map proved it, and the product-facing docs had not absorbed the patch. Doc 10 had one paragraph on objective modes and nothing on human-required continuations, quality-floor infeasibility, the acceptance verdict, reviewer findings or organization controls; doc 24 had no mention of policy/registry/statistics bundle distribution or routing telemetry; doc 19 predated slot tables and version pins; docs 17/56/83 did not mention the reviewer profile or the acceptance gate. |
| Current behavior | A build agent could implement EPR-005/007/010 to spec and still ship a UI, cloud API and definition of done that ignore the patch. |
| Replacement | Doc 10 "Execution policy in the product": objective modes, visible phases, needs-attention reasons, Review contents, label policy, organization controls. Doc 32 `execution-policy/` module and attention reasons. Doc 24 signed bundle distribution, offline freshness rule and routing telemetry path. Doc 19 slot table, versions and review lease in durable state. Doc 17 reviewer projection. Doc 56 review-isolation conformance suite. Doc 83 execution-policy acceptance. Doc 27 §28 maps all 28 v1.0 and 22 v1.1 sections; D8 enforces the map. |
| Migration | Documentation only; no graph live state changes other than DOC-GOV-004. |
| Compatibility | Consistent with ADR-R-039..056, docs 06/27/38/49/61 and the owner table; no owner moves. |
| Security impact | None on the product. The cloud doc now states that clients never receive routing weights, statistics or credentials, and that telemetry excludes solver hidden reasoning. |
| Test impact | One test added: removing a coverage-map row makes D8 fail; docs 10/24/32 mention the quality floor. |
| Rollback | Revert the change and seal commits on `main`, or restore the inventory below from `baseline.json`, and rerun the reseal. Requires another Decision Record. |
| Explicit user approval | Given in chat on 2026-09-05 (consolidate to main; complete the patches into the docs, revise the architecture, enhance the application overall). |

### Stage applicability

| Stage | DOC-GOV-004 execution |
|---|---|
| AUDITING | Section-by-section coverage audit of both patches against the docs; read docs 10/17/19/24/32/56/83; baseline hashes |
| IMPLEMENTING | Doc text, coverage map, check D8, test, this entry |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite including the new test |
| E2E_PROVEN | Change commit on `main` pushed; evidence bundle retained |
| COMPLETE | Status set through the one-step ladder with evidence; seal commit |
| Product provider/sandbox/SQLite/Git qualification | Non-applicable: no product source exists and no product claim is made |

### Status and handoff

- **Task and requirements:** DOC-GOV-004 under DR-GOV-2026-09-05-004. Docs 40/41/42/49/61, both root patches and every prior evidence bundle are byte-identical to the baseline; doc 27 sections 1–27 are unchanged and §28 is appended.
- **Revision:** repository `https://github.com/moss101/modbit.git`, branch `main`, baseline commit in `baseline.json`. Content revision: `source_revision_sha256` in `../evidence/dossier-gov-004/validation.json`. The change commit hash is recorded as `commit:` evidence on the DOC-GOV-004 node; the seal commit changes only graph live state and manifests.
- **Interfaces:** graph schema 1.2; nodes DR-GOV-2026-09-05-004 and DOC-GOV-004; check D8; no tool command changes.
- **Evidence:** `../evidence/dossier-gov-004/baseline.json`, `../evidence/dossier-gov-004/tests.log`, `../evidence/dossier-gov-004/validation.json`.
- **Checks:** Python syntax parse of all six tools; `python3 tools/test_dossier.py` (33 tests) all passed; `build_manifest`, `build_graph`, `build_manifest`, `check_dossier --manifest` all exit 0; exact outputs in validation.json.
- **Faults exercised:** missing coverage-map row rejected by D8, plus the full prior negative suite.
- **Remaining dossier acceptance:** none after the final integrity check.
- **Remaining product work/blockers:** unchanged. Production source is absent; all milestones and EPR-000..019 remain NOT_STARTED; numerical EPR thresholds still need approved measured profiles.
- **Next safe action:** `python3 tools/graph.py ready` and take M0.1; the specification is complete for that start.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `evidence/dossier-gov-004/baseline.json` |
| Added | `evidence/dossier-gov-004/tests.log` |
| Added | `evidence/dossier-gov-004/validation.json` |
| Changed | `MANIFEST.md` |
| Changed | `README.md` |
| Changed | `SKILLS.md` |
| Changed | `docs/00_MASTER_INDEX.md` |
| Changed | `docs/10_PRODUCT_PRD_AND_UX.md` |
| Changed | `docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` |
| Changed | `docs/19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` |
| Changed | `docs/24_CLOUD_CONTROL_PLANE_AND_SYNC.md` |
| Changed | `docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` |
| Changed | `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md` |
| Changed | `docs/56_TOOL_CAPABILITY_CONFORMANCE.md` |
| Changed | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| Changed | `docs/83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` |
| Changed | `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `graph/PROJECT_GRAPH.md` |
| Changed | `graph/project-graph.json` |
| Changed | `manifest.json` |
| Changed | `tools/build_graph.py` |
| Changed | `tools/check_dossier.py` |
| Changed | `tools/test_dossier.py` |

## DOC-PX-001 — Product extension stage A: authority, PX ledger tooling, phased releases, governance tiering

### Identity and authority

- Task: DOC-PX-001 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-004 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-09-05 in `07_PRODUCT_EXTENSION_DECISION_RECORD.md`, approved by the user on 2026-09-05 with twelve numbered decisions.
- Scope: doc 07; the additive ledger doc 62 with its first row (headless CLI, Alpha); doc 75 phased releases; governance tiering by behavioral risk in docs 50/83/85/86/90/93 and `../AGENTS.md`; `tools/dossier_px.py`; release nodes, `includes`/`requires_gate` edges, `graph.py releases` and `ready --release`; checks D9 and G8; computed effective totals in the check summary; three tests; pointers in docs 00/43/46/47/74/98, `../README.md`, `../SKILLS.md`. No base row, owner, ADR, EPR row or product status changes.
- Revision before change: `../evidence/dossier-px-001/baseline.json`.

### Decision Record fields

Recorded in doc 07 for the whole extension; this entry adds only: **Test impact** three tests (ledger structure, release derivation, release membership); **Rollback** revert this stage's two commits on `main`.

### Stage applicability

| Stage | DOC-PX-001 execution |
|---|---|
| AUDITING | Owner distribution of M0/M1/M2/M4 work read from the graph to ground Alpha membership; templates 85/86/90 read for tiering |
| IMPLEMENTING | Parser, builder, graph tool, checks, tests, docs 07/62/75, tiering text, pointers |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** graph schema 1.2; node type `release`; edge types `includes`, `requires_gate`; commands `releases`, `ready --release`; checks D9, G8; the check summary now prints base plus EPR plus PX effective totals.
- **Evidence:** `../evidence/dossier-px-001/baseline.json`, `tests.log`, `validation.json`; change commit recorded as `commit:` evidence on DOC-PX-001.
- **Remaining product work:** unchanged; all releases NOT_READY; all work items NOT_STARTED.
- **Next safe action:** stage B (DOC-PX-002): doc 29 client surfaces and source control with its ledger rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/07_PRODUCT_EXTENSION_DECISION_RECORD.md` |
| Added | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` |
| Added | `docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md` |
| Added | `tools/dossier_px.py` |
| Added | `evidence/dossier-px-001/baseline.json`, `tests.log`, `validation.json` |
| Changed | `AGENTS.md`, `MANIFEST.md`, `README.md`, `SKILLS.md`, `manifest.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md`, `docs/47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md`, `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`, `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`, `docs/83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`, `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md`, `docs/86_TASK_CARD_TEMPLATE.md`, `docs/90_PR_CHANGE_EVIDENCE_TEMPLATE.md`, `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md`, `docs/98_BUILD_MANIFEST.md` |
| Changed | `graph/PROJECT_GRAPH.md`, `graph/project-graph.json`, `tools/build_graph.py`, `tools/build_manifest.py`, `tools/check_dossier.py`, `tools/graph.py`, `tools/test_dossier.py` |

## DOC-PX-002 — Product extension stage B: client surfaces and source-control integration

### Identity and authority

- Task: DOC-PX-002 (`dossier_task`); owner: governance; prerequisite: DOC-PX-001 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-09-05 items 4, 5 and 10.
- Scope: new doc 29; ledger rows REQ-PX-001..013 with task cards, qualifications and PX-E2E scenarios (ten ADOPT, three DEFERRED); protocol commands and events in doc 30; `forge.*` tool family in doc 17; inline patch and PR wording in doc 20; crate placement in doc 12; small edits in docs 10/24/32; desktop and external-tools spec-doc links in the builder; one test; index entry. No base row, owner, ADR, EPR row or product status changes.
- Revision before change: `../evidence/dossier-px-002/baseline.json`.

### Stage applicability

| Stage | DOC-PX-002 execution |
|---|---|
| AUDITING | Read docs 10/12/17/20/24/30/32 anchors; confirmed no existing forge, adapter or inline-patch specification |
| IMPLEMENTING | Doc 29, ledger rows, protocol and inventory additions, placement, pointers, test |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** ledger grows to fourteen rows; releases now include PX-001 in ALPHA, PX-002/004/005/006/007/010 in BETA, PX-008/009/011 in RELEASE_ZERO; DEFERRED rows REQ-PX-003/012/013 have qualifications and no tasks.
- **Evidence:** `../evidence/dossier-px-002/` bundle; change commit recorded as `commit:` evidence on DOC-PX-002.
- **Next safe action:** stage C (DOC-PX-003): docs 28 and 63, agent competence contracts and benchmark protocol.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`; `evidence/dossier-px-002/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`, `docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`, `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`, `docs/24_CLOUD_CONTROL_PLANE_AND_SYNC.md`, `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`, `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |

## DOC-PX-003 — Product extension stage C: agent competence contracts and benchmarks

### Identity and authority

- Task: DOC-PX-003 (`dossier_task`); owner: governance; prerequisite: DOC-PX-002 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-09-05 item 6.
- Scope: new docs 28 and 63; ledger rows REQ-PX-014..021 with cards, qualifications and PX-E2E scenarios (all ADOPT; five in Alpha, two in Beta, one in Release Zero); RunStep types in doc 13; runtime-loop paragraph in doc 14; events in doc 30; `repair_attempts` table in doc 31; pointer in doc 53; spec-doc links for core-runtime, verification, context-engine and eval-bench; one test; index entries.
- Revision before change: `../evidence/dossier-px-003/baseline.json`.

### Stage applicability

| Stage | DOC-PX-003 execution |
|---|---|
| AUDITING | Read docs 13/14/30/31/33/53 for existing planning, verification-plan and RunStep text; no repair-attempt or competence-benchmark specification existed |
| IMPLEMENTING | Docs 28/63, ledger rows, domain/protocol/storage additions, pointers, test |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** ledger grows to twenty-two rows; Alpha now includes PX-014/016/017/018/019; Beta adds PX-015/020; Release Zero adds PX-021.
- **Evidence:** `../evidence/dossier-px-003/` bundle; change commit recorded as `commit:` evidence on DOC-PX-003.
- **Next safe action:** stage D (DOC-PX-004): doc 39 UX flows, onboarding and interaction budgets.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`, `docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`; `evidence/dossier-px-003/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md`, `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md`, `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`, `docs/31_DATABASE_AND_STORAGE_SCHEMA.md`, `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |

## DOC-PX-004 — Product extension stage D: UX flows, onboarding and interaction budgets

### Identity and authority

- Task: DOC-PX-004 (`dossier_task`); owner: governance; prerequisite: DOC-PX-003 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-09-05 item 7.
- Scope: new doc 39; ledger rows REQ-PX-022..025 (all ADOPT; onboarding in Alpha, states and keyboard in Beta, budgets in Release Zero); acceptance criterion and flow pointer in doc 10; renderer state rules in doc 32; budget pointer in doc 53; desktop spec-doc link; one test; index entry.
- Revision before change: `../evidence/dossier-px-004/baseline.json`.

### Stage applicability

| Stage | DOC-PX-004 execution |
|---|---|
| AUDITING | Read docs 10/32/53 for existing screens, accessibility sentence and budgets; no flows, onboarding, states or notification model existed |
| IMPLEMENTING | Doc 39, ledger rows, docs 10/32/53 edits, pointers, test |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** ledger grows to twenty-six rows; Alpha adds PX-022; Beta adds PX-023/024; Release Zero adds PX-025.
- **Evidence:** `../evidence/dossier-px-004/` bundle; change commit recorded as `commit:` evidence on DOC-PX-004.
- **Next safe action:** stage E (DOC-PX-005): doc 76 language and platform support matrix.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`; `evidence/dossier-px-004/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md`, `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |

## DOC-PX-005 — Product extension stage E: language and platform support matrix

### Identity and authority

- Task: DOC-PX-005 (`dossier_task`); owner: governance; prerequisite: DOC-PX-004 COMPLETE; outside product roll-ups. Completes stages A–E of DR-PX-2026-09-05.
- Decision Record: DR-PX-2026-09-05 items 8 and 9.
- Scope: new doc 76; ledger rows REQ-PX-026..031 (all ADOPT; Alpha baseline and platform CI matrix in Alpha, tier suites and Tier A and degradation in Beta, platform promotion in Release Zero); pointer edits in docs 10/18/20/28/43/56/72/75; index, README and SKILLS updates; spec-doc links for context-engine, verification and governance; one test.
- Revision before change: `../evidence/dossier-px-005/baseline.json`.

### Stage applicability

| Stage | DOC-PX-005 execution |
|---|---|
| AUDITING | Read docs 18/20/43/56/72 for language and platform text; no tier definitions, conformance rule or platform-state separation existed |
| IMPLEMENTING | Doc 76, ledger rows, pointer edits, test |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit, then the complete gate after the final seal |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** ledger at thirty-two rows (twenty-nine ADOPT tasks, three DEFERRED); Alpha adds PX-026/030; Beta adds PX-027/028/029; Release Zero adds PX-031. Free doc numbers remaining: 08, 09, 64–69, 77–79.
- **Evidence:** `../evidence/dossier-px-005/` bundle; change commit recorded as `commit:` evidence on DOC-PX-005.
- **Remaining product work:** unchanged; every release NOT_READY; every work item NOT_STARTED; numerical EPR thresholds and competence targets await measured baselines by design.
- **Next safe action:** `python3 tools/graph.py ready --release ALPHA` and take M0.1; PX-030 follows it inside M0.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`; `evidence/dossier-px-005/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `README.md`, `SKILLS.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`, `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`, `docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`, `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `docs/56_TOOL_CAPABILITY_CONFORMANCE.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md`, `docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |

## DOC-PX-006 — Product extension stage F: verification execution mechanics, scope bounds, repair policy and agent harness contracts

### Identity and authority

- Task: DOC-PX-006 (`dossier_task`); owner: governance; prerequisite: DOC-PX-005 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-09-05-006, approved for dossier adoption; it refines DR-PX-2026-09-05 item 6. On 2026-09-05 the user asked whether the repair loop, test-execution fidelity, scope discipline and context work were strong in the dossier, received the audit below, and answered "make it stronger over all, also make agent harness strong".
- Scope: new doc 64; docs 28 (scope policy, repair policy, execution pointers), 14 (agent harness contracts), 63 (metric refinements, two-baseline rule, benchmark test-integrity rule); ledger rows REQ-PX-032..040 with cards, qualifications and PX-E2E scenarios (all ADOPT; eight in Alpha, one in Beta); pointer and schema edits in docs 10/13/17/30/31/33/43/50/53/56/72/74/98; index, README and SKILLS updates; `MILESTONE_OVERRIDES` in `../tools/build_graph.py` scheduling IMP-EV-0107 in M2, the DR-PX-2026-09-05-006 change record and the DOC-PX-006 node; one test. No base row, owner, ADR, EPR row or product status changes; docs 40/41/42 byte-identical.
- Revision before change: `../evidence/dossier-px-006/baseline.json`.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Audit on 2026-09-05 of docs 28/62/63 against docs 14/33/50/56/70/72: "targeted tests" was required by QUAL-PX-017 but no document defined how tests are selected from changed files; "diff invariants" was named in docs 14/28/62 and defined nowhere; `failure_signature` needed a structured failing check but no contract turned runner output into one; no policy said what the Verification Engine does with a flaky target-repository test; scope discipline was self-authored (a plan revision could widen scope without a bound) and the doc 63 scope metric was ambiguous between the original and the last plan; repair bounds had no defaults; REQ-EV-0107 (bounded failure evidence for repair) was scheduled in M6 by its owner label although the M2 repair loop depends on it; PX-020 was titled an M2 baseline while sitting in M3 next to the M2 routing baseline EPR-000. |
| Current behavior | A build agent could implement PX-017/018 to the letter and ship a loop that repairs flakes, cannot name failing checks stably, accepts collateral regressions, lets the agent pass by editing tests, and has no bound on scope or on idle turns. |
| Replacement | Doc 64 defines VerificationRun stages (BASELINE/TARGETED/COMPLETION/RERUN), `TestReport`/`CheckResult` with parser confidence, `failure_signature` derivation, the flake rerun and quarantine protocol, diff invariants DI-1..DI-9 with DI-3 test integrity, regression attribution, M3 impact selection and Alpha defaults. Doc 28 adds ScopePolicy against the original plan with mandatory questions, RepairPolicy defaults, reproduction-first and no-progress detection, and the COMPLETION run before acceptance. Doc 14 adds eleven harness contracts. Doc 63 adds regression attribution, flake rate, test-integrity, no-progress and selection-quality metrics, measures scope against the original plan, scores benchmark trials with DI-3 violations as failed, and separates the M2 routing baseline from the M3 competence baseline. Docs 10/13/17/30/31/33/43/50/53/56/72 carry the projections, events, tables, tool contract, roadmap acceptance, fixtures, metrics, conformance suites and risk. |
| Migration | Additive. New rows attach to M2/M3 and existing owners; IMP-EV-0107 moves from M6 to M2 through a documented builder override with docs 40/41/42 unchanged; no status changes. |
| Compatibility | Graph schema 1.2; one change record (`refines` DR-PX-2026-09-05), one `dossier_task`, nine PX triplets with their scenarios, an `imp_task.milestone_override` field, doc 64 linked from the verification and workspace-git subsystems. D9 remains unpinned; check summary totals rise mechanically. |
| Security impact | None on the product's authority model. DI-3, DI-5, DI-7 and DI-9 narrow what a change can do; the flake protocol removes an agent-writable skip path; headless questions fail closed. |
| Test impact | One test added: doc 64/14/28 needles; PX-032..034/036..040 in Alpha and PX-035 in Beta only; IMP-EV-0107 scheduled in M2 and included in Alpha; PX-033 neighborhood; DOC-PX-006 follows DOC-PX-005; and a copied-package negative case proving the override is load-bearing (removing it makes the graph builder reject the M2→M6 dependency as a cycle). |
| Rollback | Revert this stage's two commits on `main` or restore the inventory below from `baseline.json`; requires another Decision Record because governing text changes. |
| Explicit user approval | Given in chat on 2026-09-05: "make it stronger over all, also make agent harness strong". |

### Stage applicability

| Stage | DOC-PX-006 execution |
|---|---|
| AUDITING | Coverage audit of repair loop, test execution, scope, context and harness text across docs 14/28/33/50/53/56/62/63/70/72 and the graph placement of IMP-EV-0107 and PX-020; baseline hashes |
| IMPLEMENTING | Doc 64, docs 14/28/63 rewrites and additions, ledger rows, pointer and schema edits, builder override and change record, test |
| WIRED | Regenerated graph and manifests through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite including the new test and its negative case |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product source exists |

### Status and handoff

- **Interfaces:** ledger at forty-one rows (thirty-eight ADOPT tasks, three DEFERRED); Alpha adds PX-032/033/034/036/037/038/039/040 and IMP-EV-0107; Beta adds PX-035. Doc numbers now free: 08, 09, 65–69, 77–79.
- **Evidence:** `../evidence/dossier-px-006/` bundle; change commit recorded as `commit:` evidence on DOC-PX-006.
- **Remaining product work:** unchanged; every release NOT_READY; every work item NOT_STARTED; RepairPolicy, ScopePolicy, harness and verification defaults are Alpha configuration to be revisited after the PX-020 baseline, not targets.
- **Next safe action:** `python3 tools/graph.py ready --release ALPHA` and take M0.1; inside M2, PX-032/033 follow M2.8 and IMP-EV-0107.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/64_VERIFICATION_EXECUTION_CONTRACTS.md`; `evidence/dossier-px-006/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `README.md`, `SKILLS.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md`, `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md`, `docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`, `docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`, `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`, `docs/31_DATABASE_AND_STORAGE_SCHEMA.md`, `docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md`, `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`, `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md`, `docs/56_TOOL_CAPABILITY_CONFORMANCE.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`, `docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md`, `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md`, `docs/98_BUILD_MANIFEST.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |

## DOC-GOV-005 — Release Zero execution goal: derived goal command and execution plan

### Identity and authority

- Task: DOC-GOV-005 (`dossier_task`); owner: governance; prerequisite: DOC-PX-006 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-19-005, approved for dossier adoption. On 2026-09-19, with M9.3 and M9.4 sealed, the user asked for "an executable goal to complete [the] end production ready application".
- Scope: new doc 77 (`77_RELEASE_ZERO_EXECUTION_GOAL.md`); `goal` command in `../tools/graph.py` with `GOAL_EXIT`/`DEFAULT_GOAL` and exit-code propagation from `main`; the DR-GOV-2026-09-19-005 change record and DOC-GOV-005 node in `../tools/build_graph.py`; one test and one assertion block in `../tools/test_dossier.py`; pointers in docs 00/43/75/93/98, `../README.md` and `../SKILLS.md`; the M9 row of doc 98 refreshed to the sealed state (status column unchanged); this entry. No requirement row, task, qualification, owner, ADR clause, gate definition, release rule or product status changes; docs 40/41/42/49/62 byte-identical; `../AGENTS.md` untouched.
- Revision before change: `../evidence/dossier-gov-005/baseline.json`, recording `main` c2acfc1 and every package hash.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | With 359 of 401 product work items COMPLETE, no single place stated when the product is done or ordered what remains. `graph.py ready` lists only what is startable now, `releases` only counts, `gates` only the seven gates; the dependency chain to `RELEASE_ZERO` (M9 backlog → PX-020 → M10 stages → attestations) and the inputs only the owner can supply (provider and forge credentials, threshold profiles, signing identity, reference hardware) were spread over five Decision Records, doc 61 and chat. |
| Current behavior | An agent had to re-derive the end state and the order from the graph each session, and the owner-supplied inputs were only discoverable from the BLOCKED note of PX-020 and the DR files. |
| Replacement | Doc 77 defines the goal as `RELEASE_ZERO` READY plus the packaged Release Zero proof, in terms the dossier already has, and gives the exit criteria, the dated stage plan, the owner-input register and the per-step protocol. `python3 tools/graph.py goal [RELEASE] [--json]` derives the same thing live: every open included work item and unsatisfied gate (and what they transitively wait on) layered into dependency waves over `after`, milestone `depends_on` roll-ups and gate `requires_task`; blockers with their notes and the steps they hold; the next commands; exit 0 only when the release is READY, 2 while BLOCKED, 1 otherwise. Read-only: the command never writes the graph. |
| Migration | None. No node status, edge or release rule changes; one change record and one `dossier_task` node are added by the builder; doc 77 becomes a doc node with its references. |
| Compatibility | Graph schema 1.2 unchanged. Existing commands unchanged except that `main` now returns a command's exit code (every prior command returns none, so their exit code stays 0). Standard library only, Python 3.9. |
| Security impact | None on the product. Governance: the goal has no completion path outside the one-step ladder and gate attestation the tools already enforce; doc 77 forbids substituting an agent-invented value for an owner-supplied input. |
| Test impact | One test added (`test_goal_is_derived_executable_and_release_scoped`): unknown release refused; a clean-start fixture shows M0.1 at wave 0, M1.1 waiting on `milestone:M0`, EPR-GATE-G OPEN, the RELEASE_ZERO final step, exit 1 and an unchanged graph file; `--json` parses with waves, waits and startable steps; a BLOCKED M0.1 yields exit 2, the blocker line with its note and `[held by M0.1]` on its dependents; ALPHA never waits on M10 or gate G; every item COMPLETE plus all seven attestations yields READY with exit 0. The governance chain test now also checks DOC-GOV-005 follows DOC-PX-006 with its change record and both specifying docs. |
| Rollback | Revert the two commits on `main` or restore the inventory below from `baseline.json` hashes and rerun the reseal; requires another Decision Record because docs 43/75/93/98 are governing text. |
| Explicit user approval | The request itself on 2026-09-19 ("create an executable goal to complete end production ready application"). |

### Stage applicability

| Stage | DOC-GOV-005 execution |
|---|---|
| AUDITING | Live graph read: `status`, `gates`, `releases`, `ready --release RELEASE_ZERO`, the 42 open items with their `after` and milestone prerequisites, the PX-020 blocker note, the deferred-live-half Decision Records, docs 43/59/60/61/70/73/75; baseline hashes |
| IMPLEMENTING | `goal` command, doc 77, builder wiring, test, pointer edits, this entry |
| WIRED | Regenerated graph and manifests through the real CLIs; `graph.py goal` run against the regenerated graph |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite including the new test |
| E2E_PROVEN | Change commit on `main` pushed; evidence retained |
| COMPLETE | One-step ladder with evidence; seal commit |
| Product qualification | Non-applicable: no product behavior changes; the goal command reads the graph only |

### Status and handoff

- **Interfaces:** `python3 tools/graph.py goal [ALPHA|BETA|RELEASE_ZERO] [--json]`; report fields `goal`, `state`, `exit_code`, `exit_criteria`, `work_items`, `milestones`, `gates`, `blockers[{id, milestone, since, note, holds}]`, `startable_now`, `in_progress`, `attestable_now`, `steps[{wave, id, kind, status, milestone, waiting_on, held_by, title}]`, `final_step`. Doc numbers now free: 08, 09, 65–69, 78, 79.
- **Evidence:** `../evidence/dossier-gov-005/` bundle (baseline, tests.log, validation.json); change commit recorded as `commit:` evidence on DOC-GOV-005.
- **Remaining product work:** unchanged by this task: 42 open steps in ten waves; `RELEASE_ZERO` BLOCKED by PX-020 until the owner supplies provider credentials (doc 77 §5).
- **Next safe action:** `python3 tools/graph.py goal`, then take a wave-0 M9 step (M9.5 first: audit of the existing emergency-stop path, then EPR-010 and the owner-batched IMP-EV items); in parallel the owner supplies doc 77 §5 items 1–4.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/77_RELEASE_ZERO_EXECUTION_GOAL.md`; `evidence/dossier-gov-005/baseline.json`, `tests.log`, `validation.json` |
| Changed | `MANIFEST.md`, `README.md`, `SKILLS.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md`, `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md`, `docs/98_BUILD_MANIFEST.md` |
| Changed | `tools/build_graph.py`, `tools/graph.py`, `tools/test_dossier.py` |

## DOC-GOV-006 — Correct stale Release Zero goal and IMP-EV-0245 card wording

### Identity and authority

- Task: DOC-GOV-006 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-005 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-24-006, approved for dossier adoption. On 2026-09-24 the user reported two stale descriptions and asked for them to be corrected as dossier-only maintenance. Doc 77 Stage 2 still called PX-020 `BLOCKED`, and the IMP-EV-0245 task card called its feature consumer M9 work.
- Scope: descriptive text only, in doc 77 §4 Stage 2 and in the "Limitations" line of `../evidence/m4/IMP-EV-0245/TASK_CARD.md`. The task also adds the change record and DOC-GOV-006 node in `../tools/build_graph.py`, one assertion block in `../tools/test_dossier.py`, and this entry. No requirement row, disposition, owner, task, qualification, ADR clause, gate, release rule or product status changes. Docs 40/41/42/49/62, `../AGENTS.md`, every Decision Record and every locked path stay byte-identical. Doc 77 §3 is a dated snapshot of 2026-09-19 and stays as written.
- Revision before change: `../evidence/dossier-gov-006/baseline.json`, recording `main` b196ed5 and the hashes of the governed files and the corrected card.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | `python3 tools/graph.py show PX-020` reports `COMPLETE`: the task was resumed and sealed on 2026-09-21 under DR-M3-005, with live-competence run 35560128454 and evidence under `../evidence/m3/PX-020/`. `graph.py status` shows M3 `COMPLETE`, and `graph.py goal` reports 0 `BLOCKED`. Doc 77 Stage 2 still said PX-020 "is the only `BLOCKED` step". It also said PX-020's run "discharges" the deferred live halves of EPR-000/002/005, IMP-EV-0251/0253, PX-022, PX-028 and PX-002, but no live run appears in those tasks' graph evidence or `evidence.json`. The IMP-EV-0245 card said the adaptive evaluator that consumes `FailureDiagnostic.features` "is M9 work". The graph schedules that evaluator (owner "Adaptive Profile Evaluator", REQ-EV-0244/0246) as `IMP-EV-0244` and `IMP-EV-0246` in M10. |
| Current behavior | An agent reading doc 77 would believe two false things: that an owner-input blocker still holds M10, and that the deferred live halves close with PX-020. An agent reading the IMP-EV-0245 card would look for the evaluator in M9. |
| Replacement | Doc 77 Stage 2 is now titled "resolved 2026-09-21". It states how PX-020 was resolved: DR-M9-002 credentials, the DR-M3-005 seal on the internal baseline, run 35560128454 and the evidence path. It states that M3 is `COMPLETE`, so M3 no longer holds M10. It names the two parts that stay open. The first is PX-020's public SWE-bench Verified half. The second is the deferred live halves, which still carry only offline evidence under §1 condition 5; for these it notes M2.6's gateway run 35491774156 and that the doc 15 production-endpoint clause stays open under DR-M9-002. The card names IMP-EV-0244 and IMP-EV-0246 (both EXPERIMENT, M10) as the consumer and records the correction. |
| Migration | None. No node status or edge changes apart from the added change record and `dossier_task` node. |
| Compatibility | Graph schema 1.2 unchanged. Tool behavior is unchanged apart from building the two nodes. |
| Security impact | None. |
| Test impact | The governance chain test in `../tools/test_dossier.py` now also checks that DOC-GOV-006 follows DOC-GOV-005 and has its change record, both specifying docs and the governance owner. |
| Rollback | Revert the change commit, or restore the inventory below to the hashes in `baseline.json`, then rerun the reseal. |
| Explicit user approval | The request itself on 2026-09-24 ("correct this stale wording as dossier-only maintenance"). The same request asked for a DOC-* node, this entry and the full reseal, and for a `wip/` branch with a pull request instead of a direct push to `main`. |

### Stage applicability

| Stage | DOC-GOV-006 execution |
|---|---|
| AUDITING | `git log -- docs/77*` showed doc 77 untouched since DOC-GOV-005 (49cbc43), so no rebase was needed. Graph reads: `graph.py show` for PX-020 and IMP-EV-0244/0245/0246, then `graph.py status` and `graph.py goal`. Read DR-M3-005 and DR-M9-002, and checked the evidence of every task named in the old Stage 2 for a live run. Recorded baseline hashes. |
| IMPLEMENTING | The two text corrections, builder wiring, test assertion, this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Pull request from `wip/doc-gov-006-stale-goal-wording` green on hosted CI and landed on `main` |
| COMPLETE | One-step ladder with evidence citing the landing commit; seal commit |
| Product qualification | Non-applicable: no product behavior, status or evidence claim changes. The card's proof, tests and runs are untouched. |

### Status and handoff

- **Evidence:** the `../evidence/dossier-gov-006/` bundle (baseline, tests.log, validation.json with before and after hashes of the two corrected files).
- **Remaining product work:** unchanged by this task. On 2026-09-24 `RELEASE_ZERO` was `NOT_READY` with no blocker (378/401 work items `COMPLETE`); M9 rolled up `COMPLETE` with the EPR-019 seal while this change was in review, so M10 is unblocked.
- **Known stale text left in place:** doc 77 §5 item 1 still lists PX-020, M3 and M10 under "Unblocks". It is outside this task's scope and is a candidate for a later entry.
- **Next safe action:** land the pull request, then seal DOC-GOV-006. After that, run `python3 tools/graph.py goal` and take a wave-0 step.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `evidence/dossier-gov-006/baseline.json`, `tests.log`, `tests-after-merge.log`, `validation.json` |
| Changed | `docs/77_RELEASE_ZERO_EXECUTION_GOAL.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md`, `evidence/m4/IMP-EV-0245/TASK_CARD.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |

## DOC-GOV-007 — Correct the Release Zero goal's provider-credential Unblocks cell

### Identity and authority

- Task: DOC-GOV-007 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-006 COMPLETE; outside product roll-ups.
- Decision Record: DR-GOV-2026-09-25-007, approved for dossier adoption. DOC-GOV-006 left one known stale line in place, and on 2026-09-25 the user asked for it to be fixed ("also fix the §5 item 1 Unblocks line").
- Scope: descriptive text only, in the "Unblocks" cell of doc 77 §5 item 1. The task also adds the change record and DOC-GOV-007 node in `../tools/build_graph.py`, one assertion block in `../tools/test_dossier.py`, and this entry. The rest of the row (the input and how it enters) is unchanged. No requirement row, disposition, owner, task, qualification, ADR clause, gate, release rule or product status changes. Docs 40/41/42/49/62, `../AGENTS.md`, every Decision Record and every locked path stay byte-identical.
- Revision before change: `../evidence/dossier-gov-007/baseline.json`, recording `main` 26776dd (the DOC-GOV-006 seal) and the hashes of the governed files.

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Doc 77 §5 item 1 said provider credentials unblock "PX-020 and with it M3 and M10" and "the deferred halves of DR-M2-001, DR-M3-002, DR-M3-003, DR-M6-001". Four facts contradict or refine that. First, the owner supplied credentials for a compatible gateway on 2026-09-19 (DR-M9-002). Second, PX-020 sealed `COMPLETE` on 2026-09-21 under DR-M3-005, and M3 rolled up `COMPLETE`. Third, M2.6 carries the DR-M2-001 live run on the gateway (`run:gha-35491774156`), while DR-M9-002 keeps the doc 15 "production provider endpoint" clause open. Fourth, EPR-000/002/005, IMP-EV-0251/0253, PX-022, PX-028 and PX-002 (DR-M3-002, DR-M3-003, DR-M6-001) still carry only offline runs. IMP-EV-0211 is `NOT_STARTED` in M10. |
| Current behavior | The owner-input register said credentials were still needed to unblock PX-020, M3 and M10, and it did not distinguish the gateway credentials already supplied from the production-endpoint credentials still missing. |
| Replacement | The cell first says the input was supplied for a compatible gateway (z.ai `glm-5.3-flash`, DR-M9-002). It says that unblocked PX-020 and M3 (DR-M3-005), and that M2.6's DR-M2-001 half ran live on the gateway (run 35491774156). It then lists what the item is still needed for. The first is the doc 15 production-endpoint clause, which needs credentials for `api.openai.com` / `api.anthropic.com`. The second is the deferred live halves of DR-M3-002, DR-M3-003 and DR-M6-001, which may run on the gateway but have not run. The rest are IMP-EV-0211, step 1 of the Release Zero scenario and the measured evidence behind every EPR gate. |
| Migration | None. No node status or edge changes apart from the added change record and `dossier_task` node. |
| Compatibility | Graph schema 1.2 unchanged. Tool behavior is unchanged apart from building the two nodes. |
| Security impact | None. The cell names no credential value, host secret or storage location beyond what the row and DR-M9-002 already state. |
| Test impact | The governance chain test in `../tools/test_dossier.py` now also checks that DOC-GOV-007 follows DOC-GOV-006 and has its change record, both specifying docs and the governance owner. |
| Rollback | Revert the change commit, or restore the inventory below to the hashes in `baseline.json`, then rerun the reseal. |
| Explicit user approval | The request on 2026-09-25: "also fix the §5 item 1 Unblocks line". |

### Stage applicability

| Stage | DOC-GOV-007 execution |
|---|---|
| AUDITING | Doc 77 §5 item 1 read at `main` 26776dd. `graph.py show` / graph reads for PX-020, M2.6, IMP-EV-0211 and the eight tasks with deferred live halves (run evidence only). DR-M9-002 and DR-M3-005 reread. Baseline hashes recorded. |
| IMPLEMENTING | The cell rewrite, builder wiring, test assertion, this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite |
| E2E_PROVEN | Pull request from `wip/doc-gov-007-unblocks-line` green on hosted CI and landed on `main` |
| COMPLETE | One-step ladder with evidence citing the landing commit; seal pull request |
| Product qualification | Non-applicable: no product behavior, status or evidence claim changes |

### Status and handoff

- **Evidence:** the `../evidence/dossier-gov-007/` bundle (baseline, tests.log, validation.json with before and after hashes of doc 77).
- **Remaining product work:** unchanged by this task.
- **Next safe action:** land the pull request, then seal DOC-GOV-007.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `evidence/dossier-gov-007/baseline.json`, `tests.log`, `validation.json` |
| Changed | `docs/77_RELEASE_ZERO_EXECUTION_GOAL.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |

## DOC-PX-007 — Agent-first workspace: Decision Record, specification and PX-041..068

### Identity and authority

- Task: DOC-PX-007 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-007 (`decisions/DR-PX-2026-10-03-007-agent-first-workspace.md`), status **proposed**. The owner must ratify it before any of PX-041..PX-068 starts; every root row lists DOC-PX-007 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md`, the PX rows PX-041..PX-068 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner decisions of 2026-10-03 (clean-room posture accepted, agent-first workspace first, live verification) and the parity research: static teardown, a signed-in passive read of the profile and logs, and live window probes. Live observation corrected several static claims, and doc 65 uses the corrected facts. |
| Current behavior | A supervision console over a Core far ahead of it: no conversation, streamed text, terminal view, agent list, composer controls, checkpoint or approval surface next to the conversation, projects or worktree management; design tokens and primitives are empty stubs. |
| Replacement | 28 rows (22 release-critical, 6 iteration) in three phases, 108 tagged requirements (58 LIVE, 17 DISK, 28 STATIC, 5 UNVERIFIED by lead tag), an implementation audit of 16 areas, a traceability table and a register of 26 unverified items. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | ALPHA and BETA stay READY. RELEASE_ZERO grows by 28 work items because doc 75 forces ADOPT rows into it; if the owner prefers to sequence the workspace after Release Zero the alternative is to ratify with the rows re-classed DEFERRED. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's decisions of 2026-10-03 are the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-007 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

16 areas: 4 NOT-FOUND (transcript projections, the agent-first shell, projects, Git hardening), 1 SCAFFOLDED (design tokens), 1 DOCUMENTED-ONLY (`SetExecutionPreference`), 10 IMPLEMENTED-PARTIAL (provider streaming, terminal stream, window and quit lifecycle, input queue, task modes, skills listing, approvals and run modes, context accounting, checkpoints and restore, worktree lifecycle). The mechanisms beneath are production-working and reused in place: provider streaming, durable terminals, typed input, exact-intent approvals, the context inspector, checkpoints with hash preconditions and worktree primitives. No duplicate generation was found. The Git runner sets only the prompt and locale environment; no hook, fsmonitor or attributes neutralisation exists in any crate.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-041..PX-068 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** (1) ratify with RELEASE_ZERO growing by 28 items, or have the rows re-classed DEFERRED until after Release Zero; (2) whether the reference-derived concrete surface colours should be recorded now: the spec deliberately carries constraints and Modbit-chosen accent and semantic values only, not the reference's surface hex values.
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-007-agent-first-workspace.md` |
| Changed | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` (rows REQ-PX-041..068, qualifications, cards, scenarios) |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md`, `docs/decisions/README.md` |

## DOC-PX-008 — Native computer control and browser control hardening: Decision Record, specification and PX-069..076

### Identity and authority

- Task: DOC-PX-008 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-008 (`decisions/DR-PX-2026-10-03-008-native-computer-control.md`), status **proposed**. The owner must ratify it before any of PX-069..PX-076 starts; every root row lists DOC-PX-008 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `66_NATIVE_COMPUTER_CONTROL_SPECIFICATION.md`, the PX rows PX-069..PX-076 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner statement that local OS control gets its own record, REQ-EV-0075 and 0081..0090 (already ADOPT for a Computer Runtime), doc 17's `computer.*` rows, and the research: a reference computer-use extension read statically (STATIC), the capability off in the sampled profile (DISK), the native helper not present in the bundle (UNVERIFIED). |
| Current behavior | Described in the record and in section 3 of the specification. |
| Replacement | Eight release-critical rows: Computer Runtime contract, per-call exact-intent approvals that can never be allowlisted, a macOS actuator helper process, its supply chain, browser control hardening (DevTools deny list, per-call origin gate), the desktop surface, a `computer_use` subagent profile, and screenshot artifacts; 37 tagged requirements (33 STATIC, 4 UNVERIFIED), 7 unverified items. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | Graph schema 1.2 unchanged. `tools/dossier_px.py` pins no count; the EPR pins, ADR-R-039..056, gates A to G and the sealed 291/265/291 surface are unchanged. ALPHA and BETA stay READY; RELEASE_ZERO grows by the number of rows. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's statement of 2026-10-03 that this is a real goal is the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-008 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

6 areas: browser runtime PRODUCTION-WORKING; native control DOCUMENTED-ONLY; origin gate and DevTools deny list IMPLEMENTED-PARTIAL; refusal contract and latch IMPLEMENTED-PARTIAL; permission flow NOT-FOUND; actuator and supply chain NOT-FOUND.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-069..PX-076 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** (1) observation approved per grant (the record) or per call; (2) macOS only for the first release of native control.
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/66_NATIVE_COMPUTER_CONTROL_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-008-native-computer-control.md` |

## DOC-PX-009 — Workspace Editor: Decision Record, specification and PX-077..081

### Identity and authority

- Task: DOC-PX-009 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-009 (`decisions/DR-PX-2026-10-03-009-workspace-editor.md`), status **proposed**. The owner must ratify it before any of PX-077..PX-081 starts; every root row lists DOC-PX-009 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `67_WORKSPACE_EDITOR_SPECIFICATION.md`, the PX rows PX-077..PX-081 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner statement; sealed constraints MOD-SURF-001 and MOD-SURF-002 (LOCKED), MOD-IDE-001 and MOD-IDE-002 (REJECTED), docs 20, 29 and 32, PX-005; the research gives no editor mechanics (the reference is a fork of a full IDE), so the design is independent and its requirements are tagged UNVERIFIED (design choice). |
| Current behavior | Described in the record and in section 3 of the specification. |
| Replacement | Five release-critical rows: editor file service, language intelligence from the Core, the editor surface with an engine admitted by dependency admission, agent-aware editing, a text-safety conformance suite; 21 requirements (20 UNVERIFIED design, 1 DISK); a scoped supersession of MOD-IDE-002 and MOD-SURF-002 recorded explicitly. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | Graph schema 1.2 unchanged. `tools/dossier_px.py` pins no count; the EPR pins, ADR-R-039..056, gates A to G and the sealed 291/265/291 surface are unchanged. ALPHA and BETA stay READY; RELEASE_ZERO grows by the number of rows. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's statement of 2026-10-03 that this is a real goal is the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-009 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

6 areas: Review surface and inline patch PRODUCTION-WORKING; editing view NOT-FOUND; language intelligence and editor context bridge and merge basis IMPLEMENTED-PARTIAL.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-077..PX-081 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** (1) whether completion lists and inline predictive completion are wanted in the first release; (2) explicit acceptance of the scoped change to a LOCKED row (MOD-SURF-002).
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/67_WORKSPACE_EDITOR_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-009-workspace-editor.md` |

## DOC-PX-010 — Automations: Decision Record, specification and PX-082..086

### Identity and authority

- Task: DOC-PX-010 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-010 (`decisions/DR-PX-2026-10-03-010-automations.md`), status **proposed**. The owner must ratify it before any of PX-082..PX-086 starts; every root row lists DOC-PX-010 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `68_AUTOMATIONS_SPECIFICATION.md`, the PX rows PX-082..PX-086 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner statement; MOD-AUTO-001 and REQ-EV-0149, 0264 (DEFERRED), REQ-EV-0236 (REJECT), REQ-EV-0275 and doc 81 (no second scheduler); research: the reference's automations bundle read statically (STATIC), no live observation possible (DISK: gate on, no definitions). |
| Current behavior | Described in the record and in section 3 of the specification. |
| Replacement | Five release-critical rows: definitions with enable approval bound to the definition hash, trigger evaluation inside the existing Scheduler, unattended run policy that fails closed, cloud triggers with signed webhooks, the surface and CLI; 23 requirements (17 STATIC, 6 UNVERIFIED), 4 unverified items; scoped supersession of MOD-AUTO-001 recorded, base rows left byte-identical. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | Graph schema 1.2 unchanged. `tools/dossier_px.py` pins no count; the EPR pins, ADR-R-039..056, gates A to G and the sealed 291/265/291 surface are unchanged. ALPHA and BETA stay READY; RELEASE_ZERO grows by the number of rows. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's statement of 2026-10-03 that this is a real goal is the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-010 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

6 areas: Scheduler and admission PRODUCTION-WORKING; forge webhook intake PRODUCTION-WORKING; read-only posture PRODUCTION-WORKING; unattended policy IMPLEMENTED-PARTIAL; time triggers NOT-FOUND; definitions and UI NOT-FOUND.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-082..PX-086 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** (1) whether local schedules should fire with the app closed; (2) the schedule floor and the approval expiry defaults.
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/68_AUTOMATIONS_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-010-automations.md` |

## DOC-PX-011 — Extension marketplace and Customize: Decision Record, specification and PX-087..093

### Identity and authority

- Task: DOC-PX-011 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-011 (`decisions/DR-PX-2026-10-03-011-extension-marketplace-and-customize.md`), status **proposed**. The owner must ratify it before any of PX-087..PX-093 starts; every root row lists DOC-PX-011 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `69_EXTENSION_MARKETPLACE_AND_CUSTOMIZE_SPECIFICATION.md`, the PX rows PX-087..PX-093 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner statement and hard carry-overs; REQ-EV-0225, 0138, 0137, 0114, 0224, 0239, 0240, REQ-EV-0216 (REJECT); doc 10's P0 marketplace non-goal; research: Customize editor observed live in a signed-out instance (LIVE), plugins, marketplace, MCP, hooks and security jewels read statically (STATIC). |
| Current behavior | Described in the record and in section 3 of the specification. |
| Replacement | Seven release-critical rows: package and signed catalog format, digest-pinned install and update pipeline, variables and hostile-content handling, MCP trust by configuration hash, the agent self-extension write-protection map and organisation policy, hook contract completion, the Customize surface; 18 requirements (16 STATIC, 2 LIVE), 5 unverified items. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | Graph schema 1.2 unchanged. `tools/dossier_px.py` pins no count; the EPR pins, ADR-R-039..056, gates A to G and the sealed 291/265/291 surface are unchanged. ALPHA and BETA stay READY; RELEASE_ZERO grows by the number of rows. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's statement of 2026-10-03 that this is a real goal is the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-011 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

7 areas: extension package, signature and quarantine PRODUCTION-WORKING; skills registry and importers PRODUCTION-WORKING; external-server trust, hook bus and self-extension boundary IMPLEMENTED-PARTIAL; catalog, install, update, variables and Customize NOT-FOUND.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-087..PX-093 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** (1) whether Modbit operates an official catalog; (2) which declared-capability changes force re-approval on update.
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/69_EXTENSION_MARKETPLACE_AND_CUSTOMIZE_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-011-extension-marketplace-and-customize.md` |

## DOC-PX-012 — Desktop shell platform integration (the Electron main-process change): Decision Record, specification and PX-094..098

### Identity and authority

- Task: DOC-PX-012 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-03-012 (`decisions/DR-PX-2026-10-03-012-desktop-shell-platform-integration.md`), status **proposed**. The owner must ratify it before any of PX-094..PX-098 starts; every root row lists DOC-PX-012 as a prerequisite so the graph enforces that.
- Scope: the record, its specification `78_DESKTOP_SHELL_PLATFORM_INTEGRATION_SPECIFICATION.md`, the PX rows PX-094..PX-098 in doc 62, the scoped supersession entries in docs 02 and 03, the change record and dossier task in `tools/build_graph.py`, this entry and the retained evidence. No requirement row of the sealed base, EPR ledger or ADR set, no pinned count and no product status changes.
- Revision before change: `../evidence/dossier-px-007/baseline.json` (`main` 5fdb47f, hashes of every governed file, initial integrity gate exit 0).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Owner statement without a scope; synthesis section 5; MOD-DESK-001; research on window chrome, vibrancy, permissions, deep links and process graph (LIVE, STATIC, DISK) with no source for fuses, ASAR integrity or shell updates (UNVERIFIED, stated). The record states its assumption and asks the owner. |
| Current behavior | Described in the record and in section 3 of the specification. |
| Replacement | Five release-critical rows: window chrome, native menus and deep links, main-process hardening, update application, version policy; 19 requirements (7 UNVERIFIED, 4 LIVE, 5 STATIC, 3 DISK), 6 unverified items. Renderer isolation, CSP and preload stay byte-identical. |
| Migration | None for existing nodes. Additive rows, change record and dossier task; statuses untouched. |
| Compatibility | Graph schema 1.2 unchanged. `tools/dossier_px.py` pins no count; the EPR pins, ADR-R-039..056, gates A to G and the sealed 291/265/291 surface are unchanged. ALPHA and BETA stay READY; RELEASE_ZERO grows by the number of rows. |
| Security impact | None on the product (dossier only). The record states the security model each row must prove. |
| Test impact | `tools/test_dossier.py` gains a chain assertion for the new dossier tasks and change records; no pinned constant changes. |
| Rollback | Revert the commits adding the rows and spec and rerun the reseal. Docs 02, 62 and the decisions directory are locked paths, so reversal needs a new Decision Record. |
| Explicit user approval | Pending. The owner's statement of 2026-10-03 that this is a real goal is the basis, not the ratification. |

### Stage applicability

| Stage | DOC-PX-012 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 01, 02, 03, 10, 29, 46, 62, 81, 93, 95, 96, 97, the decisions README, two recent Decision Records, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); research read from the owner's untracked archive (clean-room); implementation audit of the renderer, the main process, `surface.proto` and the Core at `main` 5fdb47f; baseline hashes in `evidence/dossier-px-007/baseline.json`. |
| IMPLEMENTING | The Decision Record, the specification, the PX rows, the supersession entries, the graph change record and dossier task, and this entry |
| WIRED | Graph and manifests regenerated through the real CLIs |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass locally |
| E2E_PROVEN | Not reached: a pull request green on hosted CI and landed on `main` after the owner ratifies the record |
| COMPLETE | Not reached: one-step ladder with evidence citing the landing commit and the owner's acceptance of the record |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes; every PX row is NOT_STARTED. |


### Implementation audit headline

6 areas: window and renderer hardening IMPLEMENTED-PARTIAL; menus and notifications IMPLEMENTED-PARTIAL; window chrome, fuses, updater and version policy NOT-FOUND.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-007/` bundle (baseline, tests.log, validation.json). One shared bundle covers DOC-PX-007 to DOC-PX-012, which were produced in one dossier change; validation.json lists each task.
- **Remaining product work:** all of PX-094..PX-098 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner questions:** What the owner means by the Electron change (the record assumes platform integration and hardening, not a framework replacement); local schedules with the app closed; whether translucency is wanted.
- **Next safe action:** the owner ratifies or amends the record (flip `status` to `accepted` with who and where in the commit that lands the rows); until then no row may start. After landing, `python3 tools/graph.py ready` lists the first rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/78_DESKTOP_SHELL_PLATFORM_INTEGRATION_SPECIFICATION.md` |
| Added | `docs/decisions/DR-PX-2026-10-03-012-desktop-shell-platform-integration.md` |

### Shared file inventory (DOC-PX-007 to DOC-PX-012)

| Action | Path |
|---|---|
| Added | `evidence/dossier-px-007/baseline.json`, `tests.log`, `validation.json` |
| Changed | `docs/02_AUTHORITY_AND_DECISIONS.md` (proposed decisions and scoped supersession table), `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` (proposed supersessions), `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`, `docs/29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md` (pointers) |
| Changed | `docs/97_DOSSIER_MAINTENANCE_LOG.md`, `tools/build_graph.py`, `tools/test_dossier.py` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |

## DOC-PX-007, DOC-PX-008 and DOC-PX-010 — Ratification of three Decision Records (2026-10-05) and the PX-067 delivered-in-part note

### Identity and authority

- Tasks: DOC-PX-007, DOC-PX-008 and DOC-PX-010 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE (unchanged); outside product roll-ups.
- Decision Records: DR-PX-2026-10-03-007 (agent-first workspace, PX-041..068), DR-PX-2026-10-03-008 (native computer control and browser hardening, PX-069..076) and DR-PX-2026-10-03-010 (automations, PX-082..086), flipped from **proposed** to **accepted** with `approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03`. On 2026-10-05 the owner set the goal "end to end implementation for tasks in research/audit/01-TASK-LIST.md", whose Tier 1 and Tier 3 build tasks need the agent-first workspace (BLD-01..07), native computer control (BLD-22) and automations (BLD-25); the instruction is recorded as the ratification of those three records, as drafted.
- **Not ratified:** DR-PX-2026-10-03-009 (Workspace Editor), -011 (extension marketplace and Customize) and -012 (desktop shell platform integration). The owner did not ask for them (the task list names a plugin marketplace as out of scope until the owner asks, and no BLD task needs the editor or the Electron main-process work). Their records stay `proposed`, their dossier tasks DOC-PX-009, DOC-PX-011 and DOC-PX-012 stay at REAL_TESTING, and PX-077..081, PX-087..093 and PX-094..098 stay blocked behind them. `graph.py set` now refuses E2E_PROVEN and COMPLETE on a dossier task whose change record is still PROPOSED, and a test asserts that no row of an accepted record depends, directly or transitively, on a row or task of a proposed one.
- Scope: the three records' front matter and approval sections; the authority lines of docs 65, 66 and 68; docs 02, 03 and 10 (proposed text now in force for the three records, still proposed for the other three); the doc 62 introduction; the PX-067 card; the decisions README ledger; `tools/build_graph.py` (the change record status follows the record file and an accepted record must name its approval); `tools/graph.py` (the guard above); `tools/test_dossier.py`; this entry; the retained evidence under `../evidence/dossier-px-007/ratification.json`, `../evidence/dossier-px-008/` and `../evidence/dossier-px-010/`. No requirement row, ADR, EPR row, gate or pinned count changes.
- Revision before change: `../evidence/dossier-px-008/baseline.json` and `../evidence/dossier-px-010/baseline.json` (commit `de1507ee`, the drafts as proposed).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | The owner's goal of 2026-10-05 (implement `research/audit/01-TASK-LIST.md`); the audit (`research/audit/00-SUMMARY.md`) shows the Core is reachable only through a supervision console and that native computer control and automations are not built. |
| Current behavior | Three accepted-pending records whose rows cannot start; BLD-01..07, BLD-22 and BLD-25 have no ratified owner row. |
| Replacement | The three records are accepted as drafted. That includes the drafts' own answers to their owner questions (observation approved per grant and never allowlistable; macOS only for the first release of native control; local schedules fire only while the Core runs; five-minute schedule floor and 24-hour approval expiry as policy data) and the release effect of DR-007 (its 28 rows stay ADOPT in RELEASE_ZERO, not re-classified DEFERRED). The owner may change any of these by a later record that names the old one in `supersedes`. |
| Migration | None for existing nodes. The three change records become APPROVED; DOC-PX-007, -008 and -010 move REAL_TESTING to E2E_PROVEN to COMPLETE through `graph.py`; PX rows stay NOT_STARTED. |
| Compatibility | ALPHA and BETA stay READY; RELEASE_ZERO already includes the rows. The scoped supersession of MOD-AUTO-001 (docs 02 and 03) and the clarifications of docs 10 and 22 take effect for 008 and 010; those of 009, 011 and 012 do not. |
| Security impact | None on the product (dossier only). Accepting 008 and 010 adds no capability by itself; every row still needs its own real proof. |
| Test impact | `tools/test_dossier.py` replaces the all-proposed assertion with per-record states and adds two tests: unratified records stay blocked and no accepted row depends on them; record status and approval match the graph. |
| Rollback | Restore the three records to `status: proposed` and `approved_by: pending ...` by a new Decision Record (docs/62 and the decisions directory are locked paths) and rerun the reseal. |
| Explicit user approval | The owner's instruction of 2026-10-05 in chat, quoted above. |

### PX-067 delivered-in-part note

The audit fix FIX-01 (pull request moss101/modbit#61, branch `wip/audit-fixes-integration`, not merged) delivered most of PX-067's scope. A clearly marked note on the PX-067 card and in doc 65 lists the remaining scope (bare-repository safeguard, redaction beyond error text, the security event, the architecture-lint rule and the spawns outside `crates/git`, repository-local `core.sshCommand` and `credential.helper`, and QUAL-PX-067 on the Core operations on the packaged build). The row text, prerequisites, tier and qualification are unchanged and the row stays NOT_STARTED; nothing of it earns status until #61 is merged and the qualification runs. The note is an annotation under DR-PX-2026-10-03-007.

### Stage applicability

| Stage | Execution |
|---|---|
| AUDITING / IMPLEMENTING / WIRED | Done for the original drafts (entries above); here: the ratification edits, the builder and tool changes, the regenerated graph and manifests |
| REAL_TESTING | Held already; re-established by `check_dossier --manifest` (exit 0) and the copied-package suite (exit 0) on the ratified tree |
| E2E_PROVEN | For a dossier task: the integrity and copied-package suites pass on the ratified tree and the owner's acceptance is recorded. Hosted-CI landing of the branch on `main` has NOT happened (no push or merge was performed); the landing commit is added later as further `commit:` evidence with `graph.py set ... COMPLETE --evidence commit:<sha>`, which the tool allows at the same state |
| COMPLETE | The owner's acceptance is the only remaining precondition of these three tasks, and it is recorded |
| Product qualification | Non-applicable: dossier only. No product behaviour, status or evidence claim changes |

### Status and handoff

- **Evidence:** `../evidence/dossier-px-007/ratification.json`, `../evidence/dossier-px-008/{baseline.json,tests.log,validation.json}`, `../evidence/dossier-px-010/{baseline.json,tests.log,validation.json}` and the change commit as `commit:` evidence on each graph node.
- **Remaining dossier acceptance:** DOC-PX-009, DOC-PX-011 and DOC-PX-012 await the owner. Landing of this branch on `main` through a green hosted-CI pull request.
- **Remaining product work:** every PX row NOT_STARTED. Nothing here is a product proof.
- **Next safe action:** `python3 tools/graph.py ready` lists the rows of the three accepted records whose other prerequisites are complete; DR-PX-2026-10-05-013 (next entry) adds the audit-driven rows.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `evidence/dossier-px-007/ratification.json`, `evidence/dossier-px-008/*`, `evidence/dossier-px-010/*` |
| Changed | `docs/decisions/DR-PX-2026-10-03-007-agent-first-workspace.md`, `-008-native-computer-control.md`, `-010-automations.md`, `docs/decisions/README.md` |
| Changed | `docs/02_AUTHORITY_AND_DECISIONS.md`, `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`, `docs/10_PRODUCT_PRD_AND_UX.md`, `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`, `docs/65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md`, `docs/66_NATIVE_COMPUTER_CONTROL_SPECIFICATION.md`, `docs/68_AUTOMATIONS_SPECIFICATION.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/graph.py`, `tools/test_dossier.py` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |

## DOC-PX-013 — Audit-driven capability completion: Decision Record, specification and PX-099..139

### Identity and authority

- Task: DOC-PX-013 (`dossier_task`); owner: governance; prerequisite: DOC-GOV-007 COMPLETE; outside product roll-ups.
- Decision Record: DR-PX-2026-10-05-013 (`decisions/DR-PX-2026-10-05-013-audit-driven-capability-completion.md`), status **accepted** on 2026-10-05 with `approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03`. Every row lists DOC-PX-013 as a prerequisite so the graph enforces the order.
- Scope: the record; the specification `79_AUDIT_DRIVEN_CAPABILITY_COMPLETION_SPECIFICATION.md` (audit classification of the 30 BLD tasks, 41 requirements ADC-A01..ADC-J03, the traceability table BLD to PX rows to owner to tier to prerequisites, the corrections to the draft mapping and the non-adoptions); rows REQ-PX-099..139 in doc 62 (a ledger row, a qualification, a task card and a PX-E2E scenario each); the change record and dossier task in `tools/build_graph.py`; the pinned parity counts and the dependency-separation guard in `tools/test_dossier.py`; this entry; the retained evidence under `../evidence/dossier-px-013/`. No sealed row, EPR row, gate, ADR-R clause, pinned count or product status changes.
- Revision before change: `../evidence/dossier-px-013/baseline.json` (the ratified tree of the previous entry).

### Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | The owner's goal of 2026-10-05 and the audit (`research/audit/00-SUMMARY.md`, `01-TASK-LIST.md`, `audit-A..I.md`, and the fix results in `02-PHASE0-AND-FIX-RESULTS.md`). |
| Current behavior | Of the 30 BLD tasks, four are fully covered by accepted rows (BLD-01, 06, 22, 25), eight are covered in their principal part (BLD-02, 03, 04, 05, 07, 17, 18, 19) and eighteen have no row (BLD-08 to 16, 20, 21, 23, 24, 26 to 30). |
| Replacement | 41 rows in ten groups over existing owners, 39 release-critical and 2 iteration (PX-137 and PX-138, measurement only), each naming its real boundary, failure injection and mutation; doc 79 as the specification and traceability. Eight corrections to the draft mapping are recorded (doc 79 section 8). |
| Migration | Additive. The graph gains a change record, DOC-PX-013 and 41 PX nodes. RELEASE_ZERO grows by 41 work items; ALPHA and BETA are unchanged. |
| Compatibility | No row depends on a row or task of the proposed records DR-PX-2026-10-03-009, -011 or -012 (asserted by a test). PX-106 is a Customize view over existing registries only; PX-117 does not depend on PX-092. EPR-pinned behaviour is untouched: PX-133 conforms the statistics emission to ADR-R-051 and ADR-R-055, PX-134 adds operator verbs for the signed registry, PX-135 widens calibration data, PX-136 measures; reviewer family enforcement, a correlated feasibility bound and a no-checks cascade and critique policy are NOT adopted (doc 79 section 9). |
| Security impact | None on the product (dossier only). Every row carries its hostile-input or fault proof. |
| Test impact | `tools/test_dossier.py` pins the parity surface (PX-000..139: 140 rows, 137 ADOPT tasks, 3 DEFERRED; seven records of which four accepted; 41 rows here, tier split 39 and 2), asserts that each of the 41 rows has an existing owner, DOC-PX-013 and the record's authorization, and that doc 79 traces every BLD task and every new row; the generic guards for accepted-versus-proposed records apply to the new record. |
| Rollback | Revert the commits adding the rows, the specification and the graph nodes and rerun the reseal; a new record is needed because doc 62 and the decisions directory are locked paths. |
| Explicit user approval | The owner's instruction of 2026-10-05 in chat, quoted above. |

### Stage applicability

| Stage | DOC-PX-013 execution |
|---|---|
| AUDITING | Governing files read (AGENTS.md, SKILLS.md, docs 02, 62, 75, 81, 83, 93, 95, 96, 97, the decisions README, DR-PX-2026-10-03-007 and -010, `tools/build_graph.py`, `dossier_px.py`, `check_dossier.py`, `test_dossier.py`); the audit read in full; every BLD task checked against the actual rows of doc 62; the audit-fix branch inspected for what FIX-01 delivered (PX-067 note). |
| IMPLEMENTING | The record, the specification, the rows, the graph change record and dossier task, the tests, this entry |
| WIRED | Graph and manifests regenerated through the real CLIs; the rows reach the graph with existing owners, M10, RELEASE_ZERO membership and the gating edge to this task |
| REAL_TESTING | `check_dossier --manifest` and the copied-package suite pass |
| E2E_PROVEN and COMPLETE | For a dossier task: the integrity and copied-package suites pass on the sealed tree and the owner's acceptance is recorded. Hosted-CI landing of the branch on `main` has not happened (no push or merge was performed) and is added later as further `commit:` evidence |
| Product qualification | Non-applicable: dossier only. Every PX row is NOT_STARTED and none is a product proof |

### Implementation audit headline

30 BLD tasks classified at `main` 5fdb47f1 (doc 79 section 4): 4 fully covered by accepted rows; 8 partly covered with the residual specified here; 18 not covered. The audit fixes (FIX-01..21, unmerged, pull request #61) already delivered parts that the rows build on; rows that depend on them say so, because fix tasks are not graph nodes and cannot gate a row.

### Status and handoff

- **Evidence:** the `../evidence/dossier-px-013/` bundle (baseline, tests.log, validation.json) and the change commit as `commit:` evidence on the graph node.
- **Remaining product work:** all of PX-099..139 NOT_STARTED. Unfinished behaviour: all of it. Nothing here is a product proof.
- **Owner inputs the rows name:** a dependency admission for the embedding model (PX-112); a GitHub test repository and token (PX-125 to PX-127); first-party provider keys (PX-136); a threshold profile for gates A to G (no row attests a gate); the signing identity and SBOM record of M10.2.
- **Next safe action:** `python3 tools/graph.py ready` lists the rows whose prerequisites are complete; take one, set it AUDITING and begin with the existing-code audit on the merged state. Merging pull request #61 first removes the main ambiguity about what the fixes delivered.

### Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/79_AUDIT_DRIVEN_CAPABILITY_COMPLETION_SPECIFICATION.md`, `docs/decisions/DR-PX-2026-10-05-013-audit-driven-capability-completion.md` |
| Added | `evidence/dossier-px-013/baseline.json`, `tests.log`, `validation.json` |
| Changed | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` (rows REQ-PX-099..139, qualifications, cards, scenarios, introduction) |
| Changed | `docs/00_MASTER_INDEX.md`, `docs/02_AUTHORITY_AND_DECISIONS.md`, `docs/decisions/README.md`, `docs/97_DOSSIER_MAINTENANCE_LOG.md` |
| Changed | `tools/build_graph.py`, `tools/test_dossier.py` |
| Changed | `MANIFEST.md`, `manifest.json`, `graph/PROJECT_GRAPH.md`, `graph/project-graph.json` |
