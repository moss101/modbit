# EPR v1.1 dossier task and handoff

## Identity, intake and audit

- Task: DOC-EPR-002; owner: governance; prerequisite: DOC-EPR-001 COMPLETE; outside product milestone roll-ups.
- Authority: DR-EPR-2026-09-05-v1.1; adopted ADR-R-049..056; amend existing EPR-000..013 and add EPR-014..019 with matching requirements/qualifications.
- Scope: surgical specification supersession, current-source graph generation, validation and resealing; no product implementation claim.
- Revision before change: `../evidence/dossier-epr-v1.1/baseline.json`. No Git repository/commit exists.
- Intake: graph.py ready still selects M0.1 and existing governance IMP tasks for product work. This separately authorized dossier follow-up depends only on the completed prior dossier task.
- Required reading: AGENTS.md, SKILLS.md, docs 01/02/05/11/12/14/15/27/38/49/61/81/87/93/94/98; both complete root patches; current graph/parser/manifest/check/test tooling. Exact pre-change hashes are in the baseline.
- Existing-code classification: DOCUMENTED-ONLY product; working filesystem dossier pipeline with an expected unsealed new patch. Trace: source Markdown → shared EPR parser → graph nodes/edges/status/render → manifest hashes → integrity and copied-package negative checks.
- First missing links: active v1.0 clauses contradict v1.1; parser counts stop at 14 triplets/10 ADRs; graph has no v1.1 source/adoption/supersession edges. No product caller, provider process or database implementation is present.

## Invariants and implementation plan

Preserve both source patches and the original 291 EV rows, existing IDs/owners and prior evidence. Adopt the new Decision Record before locked amendments. Replace conflicting canonical clauses in place, preserve non-conflicting mechanisms and map six distinct delta slices onto current owners. Extend existing parser/graph/checker; do not create another runtime, authority or graph. All product tasks remain NOT_STARTED.

## Stage applicability

| Stage | DOC-EPR-002 execution |
|---|---|
| SELECT / READ / TRACE / AUDIT / MAP / PLAN | Intake, source hashes, first missing links and approved replacement map in doc 06 |
| IMPLEMENT VERTICAL SLICE | Active clauses → task/qualification/dependency metadata → graph/supersession views → sealed package |
| TEST LOCALLY / INTEGRATION | Execute actual Python CLIs and copied-package positive/negative checks |
| TEST REAL EFFECT | Real filesystem generation, current-source equivalence and SHA-256 verification of both manifest formats |
| INJECT FAILURE | Copied incomplete/cyclic/stale/conflicting authority/task/seal inputs must be rejected |
| Product provider/sandbox/SQLite/Git qualification | Non-applicable to this dossier-only task: product source/build absent; all named EPR qualifications remain future work |
| CAPTURE / UPDATE / HANDOFF | Retain validation evidence, set only dossier status via graph.py, regenerate final graph and manifests |

## Status and final handoff

DOC-EPR-002 completes the dossier pipeline only; authoritative state and evidence references are in `../graph/project-graph.json`. DOC-EPR-001 retains its prior completion and original evidence. All 363 product work items (78 milestone tasks, 265 IMP-EV tasks and 20 EPR tasks) remain NOT_STARTED. No product provider, sandbox, recovery, calibration or rollout test was run or claimed.

- **Task and requirements:** DOC-EPR-002; amended EPR/REQ-EPR/QUAL-EPR-000..013; added EPR/REQ-EPR/QUAL-EPR-014..019; 40 EPR-E2E/EPR-FI scenarios; seven activation gates A–G. The original 291 REQ-EV rows/dispositions/owners are byte-identical.
- **Decision authority:** DR-EPR-2026-09-05-v1.1, ADR-R-049..056, with ten scoped supersession links to prior ADRs in doc 02. No ID collisions, renumbering or new canonical subsystem; all 23 existing owners survive.
- **Branch/commit/revision:** direct workspace `/Users/mohsin/projects/modbit`, no Git repository or branch. Exact revision is the SHA-256 of the sorted source hash index in `../evidence/dossier-epr-v1.1/validation.json` (`source_revision_sha256`); both package manifests seal the final generated graph and evidence. A Git commit could not be created because this directory is not a Git repository.
- **Schema/interfaces:** graph schema 1.2 records both immutable patch sources, approved changes, scoped supersessions and DOC-EPR-002; dossier edition V3.3 EPR v1.1. Future product writers use ConditionalExecutionPlan schema 2, RoutingDecisionRecord, OutcomeStatistics, RealizedRisk, AcceptanceGateResult and ReviewerResult; legacy adapters remain explicit. Product schema migration is specified, not executed.
- **Real filesystem evidence:** `../evidence/dossier-epr-v1.1/baseline.json`, `../evidence/dossier-epr-v1.1/tests.log` and `../evidence/dossier-epr-v1.1/validation.json`. This is a SHA-256 integrity reseal, not a product certification or digital signature.
- **Checks:** Python syntax parse; 26 tests in `python3 tools/test_dossier.py`, all passed; actual `build_manifest.py → build_graph.py → build_manifest.py → check_dossier.py --manifest` pipeline, all exit 0. Commands, environment, outcomes and source/test digests are retained in validation.json. The graph and both manifest views are regenerated again after dossier-only completion so status and evidence hashes are current.
- **Faults exercised:** incomplete/duplicate requirements or cards, owner/qualification mismatch, explicit and implicit milestone cycles, missing ADR/supersession/statistics replay fields, obsolete primary runtime algorithm, removed independent gate calibration, immutable patch tampering even after attempted reseal, unlisted payload, stale human views/source graph, dangling/duplicate graph nodes and evidence/upstream completion bypass. Faults run in disposable package copies using the real CLIs. Live-state preservation and exclusion of dossier completion from product roll-ups also pass.
- **Remaining dossier acceptance:** none after final integrity check. No unresolved package-test failures. Both root patches and the historical doc 05/94/evidence bundle are unchanged.
- **Remaining product work/blockers:** production source/build is absent; EPR-000..019 and the full milestone path still require real implementation and their named qualifications. Numerical tau/delta, representative sample minima and independent acceptance/risk thresholds need approved measured profiles. Rollout requires applicable A–G evidence; economical initial-leg policies also require EPR-019 calibration, and review slots require EPR-018 real isolation. Unsupported reviewer sandboxes fail admission.
- **Next safe action:** run `python3 tools/graph.py ready`, take M0.1 on the critical path, audit/create the actual monorepo and CI using its task card; only after upstream proof proceed to the M2 baseline and the v1.1 dependency graph. Do not implement superseded v1.0 primary templates or mark product work complete from this seal.

## Exact file inventory for this change

The user-supplied v1.1 root patch was already present at intake; it is newly included in the manifest but remains byte-identical. No files were deleted. Source revisions and all final package bytes are identified by validation.json and manifest.json respectively.

| Action | Path |
|---|---|
| Added | `docs/06_EPR_V1_1_SUPERSESSION_DECISION.md` |
| Added | `docs/95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md` |
| Added | `evidence/dossier-epr-v1.1/baseline.json` |
| Added | `evidence/dossier-epr-v1.1/tests.log` |
| Added | `evidence/dossier-epr-v1.1/validation.json` |
| Changed | `MANIFEST.md` |
| Changed | `README.md` |
| Changed | `SKILLS.md` |
| Changed | `docs/00_MASTER_INDEX.md` |
| Changed | `docs/01_START_HERE_FOR_BUILD_AGENTS.md` |
| Changed | `docs/02_AUTHORITY_AND_DECISIONS.md` |
| Changed | `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` |
| Changed | `docs/04_REQUIREMENT_BASIS_AND_LIMITS.md` |
| Changed | `docs/11_SYSTEM_ARCHITECTURE.md` |
| Changed | `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md` |
| Changed | `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md` |
| Changed | `docs/15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` |
| Changed | `docs/18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` |
| Changed | `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` |
| Changed | `docs/23_SECURITY_POLICY_EFFECT_LEDGER.md` |
| Changed | `docs/26_SKILL_REGISTRY_AND_EVOLUTION.md` |
| Changed | `docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` |
| Changed | `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` |
| Changed | `docs/31_DATABASE_AND_STORAGE_SCHEMA.md` |
| Changed | `docs/34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` |
| Changed | `docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` |
| Changed | `docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` |
| Changed | `docs/41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` |
| Changed | `docs/42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` |
| Changed | `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` |
| Changed | `docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md` |
| Changed | `docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` |
| Changed | `docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md` |
| Changed | `docs/47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` |
| Changed | `docs/48_FEATURE_DEPTH_CONTRACTS.md` |
| Changed | `docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` |
| Changed | `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` |
| Changed | `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md` |
| Changed | `docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md` |
| Changed | `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md` |
| Changed | `docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` |
| Changed | `docs/60_RELEASE_ZERO_EXPANDED_PROOF.md` |
| Changed | `docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` |
| Changed | `docs/71_OPERATIONS_RUNBOOK.md` |
| Changed | `docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md` |
| Changed | `docs/73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` |
| Changed | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| Changed | `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` |
| Changed | `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md` |
| Changed | `docs/98_BUILD_MANIFEST.md` |
| Changed | `graph/PROJECT_GRAPH.md` |
| Changed | `graph/project-graph.json` |
| Changed | `manifest.json` |
| Changed | `tools/build_graph.py` |
| Changed | `tools/build_manifest.py` |
| Changed | `tools/check_dossier.py` |
| Changed | `tools/dossier_epr.py` |
| Changed | `tools/graph.py` |
| Changed | `tools/test_dossier.py` |
