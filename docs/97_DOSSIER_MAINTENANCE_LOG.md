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
