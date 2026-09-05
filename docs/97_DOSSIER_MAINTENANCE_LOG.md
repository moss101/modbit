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
