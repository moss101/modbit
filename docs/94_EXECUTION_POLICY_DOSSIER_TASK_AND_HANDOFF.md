# Execution policy dossier task and handoff

## Identity and intake

- Task: DOC-EPR-001; owner: governance; scope: dossier maintenance outside product milestone roll-ups.
- Decision: DR-EPR-2026-09-05; ADR-R-039..048.
- Requirements: preserve REQ-EV-0001..0291; add REQ-EPR-000..013 and EPR-000..013 with QUAL-EPR-000..013.
- Risk: architecture/recovery specification change; no product effects are executed.
- Revision: V3.1 input hash set in `../evidence/dossier-epr/baseline.json`; Git commit unavailable because this workspace has no `.git` repository.
- Intake: `python3 tools/graph.py ready` identified M0.1, IMP-EV-0208 and IMP-EV-0242 as ready. None represents this requested dossier amendment; DOC-EPR-001 is an independent governance work item with no product prerequisites, authorized by `05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md`.

## Required reading and audit

Read AGENTS.md and SKILLS.md; docs 01, 02, 11, 12, 13, 14, 15, 30, 31, 33, 34, 40–48, 53, 74, 81, 86, 87, 93, 98; the complete supplied patch; all four dossier tools. Revisions are the SHA-256 entries in the baseline evidence file.

Classification: DOCUMENTED-ONLY for product routing, PRODUCTION-WORKING for the existing dossier CLI at its filesystem boundary (baseline integrity passes). Trace: Markdown ledgers → build_graph parser → JSON nodes/edges → graph query/status/render → build_manifest hashing → check_dossier filesystem validation. First missing links: EPR contracts/tasks/ADRs are not parsed or linked; static manifest inputs omit the patch; the prescribed manifest-then-graph sequence leaves graph hashes stale. No product caller, provider executor, SQLite Run store or compiled desktop is present.

## Invariants and plan

Preserve locked EV rows and canonical owners; retain the source patch unchanged; no fabricated product proof or new scheduler; keep product task states NOT_STARTED. Add an explicit adoption record and full requirements-to-task-to-test mapping. Reconcile current runtime, policy, storage, protocol, UX, economics and verification docs. Extend the existing tooling to parse EPR tasks/gates, preserve state and reject inconsistent links/dependencies. Rebuild both graph views and hash the final artifacts.

## Stage applicability

| Stage | Application to DOC-EPR-001 |
|---|---|
| SELECT / READ / TRACE / AUDIT / MAP / PLAN | Intake, baseline audit and mapping above; product coding is not selected |
| IMPLEMENT VERTICAL SLICE | Adoption → canonical docs → requirement/task/qualification parser → query/status → manifest |
| TEST LOCALLY | Python syntax, deterministic parsing, status preservation and dependency checks |
| TEST INTEGRATION | Generate full graph and human view from real dossier, then validate all references |
| TEST REAL EFFECT | Real filesystem generation and SHA-256 verification of the delivered package |
| INJECT FAILURE | Corrupt copied task/qualification/dependency/manifest fixtures; validator must reject them |
| Product provider/worktree/SQLite/E2E execution | Non-applicable to this documentation task: no product source/build exists. Required product proofs remain outstanding on EPR tasks |
| CAPTURE / UPDATE / HANDOFF | Retained baseline and validation evidence, graph status through graph.py, final manifests and handoff below |

## Status and handoff

- **Task:** DOC-EPR-001; final authoritative lifecycle status is stored in `../graph/project-graph.json`. This handoff closes only the dossier filesystem pipeline after the checks below pass.
- **Product requirements/tasks:** all 291 REQ-EV rows and their 265 IMP-EV tasks preserved; REQ-EPR-000..013 / EPR-000..013 / QUAL-EPR-000..013 added. All product milestone and EPR implementation states remain NOT_STARTED.
- **Decision refs:** DR-EPR-2026-09-05, ADR-R-039..048. Historical destination mapping and compatibility are recorded in doc 05.
- **Revision:** V3.2 EPR, content revision and source digests in `../evidence/dossier-epr/validation.json`; no Git commit or branch exists in this workspace. Generated artifacts cannot be committed until a Git repository is established.
- **Schema/migrations:** dossier graph schema 1.1 (additive EPR, release gate, source patch, change record and dossier-task metadata). Product Run/event/protocol/SQLite migrations are specified in docs 13/30/31/38 but have not been implemented or executed.
- **Interfaces/events:** compiler/leg/gate/risk/accounting/policy activation contracts documented in docs 27/30/38. No live product API changed.
- **Tests:** `python3 tools/test_dossier.py` executes 19 real copied-package CLI tests; exact output/result and environment are retained in `../evidence/dossier-epr/tests.log` and validation.json. `python3 tools/check_dossier.py --manifest` checks 76 docs, 291/265/291 EV records plus 14 EPR triplets, 10 adopted ADRs, 7 gates, source/graph equality, dependencies and final hashes. Baseline check and Python syntax validation also passed.
- **Real effect evidence:** actual filesystem graph/Markdown generation, manifest hashing, deterministic rebuild and live-state preservation, including both human/machine manifest consistency. These are dossier proofs, not product provider or runtime qualification.
- **Negative/fault cases:** missing/duplicate requirement/task/ADR, wrong qualification or owner, explicit and implicit milestone cycles, invalid gate dependency, stale graph source/view, tampered/unlisted payload, divergent human manifest, dangling/duplicate graph IDs and evidence/upstream completion guards. Copied-fixture state has no product-evidence authority.
- **Remaining product work:** every EPR acceptance criterion in docs 49/61; real provider/Git/database execution, restart/fencing, critic denial, risk policy, full compound accounting, cache economics, isolated replay, calibrated holdout thresholds and controlled rollout/rollback. No verified cost/success/latency improvement is claimed.
- **Blockers for this dossier delivery:** none after final validation. Git commit is unavailable because `.git` is absent; no repository was initialized as a side effect of documentation work.
- **Next safe action:** run `python3 tools/graph.py ready` and begin M0.1 (monorepo/CI/protocol foundation). Once the real local engineering loop exists, select graph-ready EPR-000 and establish the direct baseline. Do not bypass the upstream milestone or EPR task prerequisites.

## Evidence → finding → development path

| Evidence | Finding | Development path |
|---|---|---|
| Baseline hashes and baseline integrity pass | Existing dossier/tooling works, EPR is absent and product code does not exist | Approved adoption record; preserve EV rows/source patch and extend existing owners |
| Source patch sections 1–28 and legacy destination audit | Old filenames do not identify current build authority | Doc 05 maps destinations; docs 27/38/49/61 hold active behavior/contracts/tasks/tests |
| Regeneration and copied-package negative tests | EPR traceability and completion/dependency guards reach the filesystem boundary | Graph-driven development with current manifest hashes; no runtime completion shortcut |

## Adoption checklist and activation limits

- [x] Preserve original source patch and register its hash/provenance in the active package.
- [x] Record superseded historical frontier-only and frontier-token-only premises without inventing absent historical ADR IDs.
- [x] Register ADR-R-039..048 and reconcile current runtime/Gateway, economics, algorithms and capability/migration documents.
- [x] Materialize EPR-001..013 plus the required direct baseline as explicit dependency-ordered tasks with requirements, qualifications, faults and rollout gates.
- [x] Keep existing Core, policy, effects, context, recovery, worktree and verification ownership; no new canonical subsystem.
- [x] Preserve all product states as NOT_STARTED and require real evidence before activation/completion.
- [x] Define early static-DIRECT qualification separately from later compound/learned/Skill policy promotion; full compound accounting follows working compound paths.
- [ ] Product DIRECT baseline, untouched holdout, workflow benefit and production rollback proofs: required future development, not part of DOC-EPR-001.

## Files added

- `../docs/05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md`
- `../docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`
- `../docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`
- `../docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`
- `../docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md`
- `../docs/94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md`
- `../evidence/dossier-epr/baseline.json`
- `../evidence/dossier-epr/tests.log`
- `../evidence/dossier-epr/validation.json`
- `../tools/dossier_epr.py`
- `../tools/test_dossier.py`

## Files changed

- `../MANIFEST.md`
- `../README.md`
- `../SKILLS.md`
- `../docs/00_MASTER_INDEX.md`
- `../docs/01_START_HERE_FOR_BUILD_AGENTS.md`
- `../docs/02_AUTHORITY_AND_DECISIONS.md`
- `../docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`
- `../docs/04_REQUIREMENT_BASIS_AND_LIMITS.md`
- `../docs/10_PRODUCT_PRD_AND_UX.md`
- `../docs/11_SYSTEM_ARCHITECTURE.md`
- `../docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`
- `../docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md`
- `../docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md`
- `../docs/15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md`
- `../docs/18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`
- `../docs/19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md`
- `../docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`
- `../docs/23_SECURITY_POLICY_EFFECT_LEDGER.md`
- `../docs/26_SKILL_REGISTRY_AND_EVOLUTION.md`
- `../docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`
- `../docs/31_DATABASE_AND_STORAGE_SCHEMA.md`
- `../docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md`
- `../docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md`
- `../docs/34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md`
- `../docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md`
- `../docs/41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md`
- `../docs/42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md`
- `../docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`
- `../docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md`
- `../docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md`
- `../docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md`
- `../docs/47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md`
- `../docs/48_FEATURE_DEPTH_CONTRACTS.md`
- `../docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`
- `../docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md`
- `../docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md`
- `../docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md`
- `../docs/54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md`
- `../docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md`
- `../docs/60_RELEASE_ZERO_EXPANDED_PROOF.md`
- `../docs/70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md`
- `../docs/71_OPERATIONS_RUNBOOK.md`
- `../docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md`
- `../docs/73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md`
- `../docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`
- `../docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`
- `../docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md`
- `../docs/98_BUILD_MANIFEST.md`
- `../graph/PROJECT_GRAPH.md`
- `../graph/project-graph.json`
- `../manifest.json`
- `../tools/build_graph.py`
- `../tools/build_manifest.py`
- `../tools/check_dossier.py`
- `../tools/graph.py`

## Files deleted

None. The original root patch and `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` remain byte-for-byte identical to the baseline.
