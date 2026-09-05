# Modbit — AI-Agent Build Dossier V3.3 EPR v1.1

> **Authority date:** 2026-09-05  
> **Purpose:** complete native Modbit implementation specification optimized for AI coding agents.  
> **Supersedes for build execution:** prior provenance-heavy dossier editions and the V3 two-part numbering. Research packages remain evidence only, not instructions to imitate another product.

## What changed in V3.3 EPR v1.1

One ConditionalExecutionPlan replaces primary workflow templates; DIRECT/CASCADE/CRITIQUE are derived path labels. Confidence-adjusted quality precedes cheapest-plan selection. Outcome Statistics is separately versioned; factual assurance and evidence acceptance are independent. Isolated Non-Committing Reviewers can run bounded disposable evidence work, never canonical/persistent/external effects. All continuations are validated/reserved before execution. Request/leg/gate attribution and independent gate calibration are release-critical.

Preserve the original 291 EV rows and prior IDs/owners; amend 14 EPR tasks and add six distinct delta slices (20 total), 40 real/fault scenarios, 18 adopted EPR ADRs and seven gates. Adoption/supersession: doc 06; current handoff/seal: doc 95. Product implementation remains NOT_STARTED.

Governance maintenance (DOC-GOV-001, doc 96, 2026-09-05): evidence references on graph nodes use a validated `kind:value` grammar, decision statuses are validated against doc 93, `../AGENTS.md`, `../README.md` and `../SKILLS.md` match the sealed package and five-step reseal, and the pinned tool constants are documented in doc 74. No requirement, owner, ADR or EPR clause changed.

Later dossier-only maintenance is logged append-only in doc 97 (`97_DOSSIER_MAINTENANCE_LOG.md`). Its first entry, DOC-GOV-002, aligns docs 12/14/16/21/33/44/74 with EPR v1.1: component placement by crate and owner, the `review_isolated` execution profile and the reviewer tool projection. DOC-GOV-003 adds `gated_by` edges from M10 to gates A–G, derived gate states with `graph.py attest` and `gates`, the `GATED` roll-up and one-step lifecycle transitions (doc 93).

## What changed in V3.1 (structure only, no requirement changes)

1. All specification files live in `docs/`; the governing files `README.md`, `AGENTS.md`, `SKILLS.md`, `MANIFEST.md` and `graph/` live at the repository root.
2. Every file has a unique number. The V3 edition reused 17–29 for two different sets of files; that ambiguity is gone. A full old→new map is recorded in `../MANIFEST.md`.
3. The V3 files numbered 28 (existing-code donor/reuse policy) and 29 (old-repo donor migration rules) were duplicates; the superset survives as `37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md`.
4. Three overlapping status ladders are reconciled in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`.
5. Leftover de-branding artifacts (doubled substrate wording, research-project shorthand in headings) were replaced with plain wording.
6. A machine-readable project graph (`../graph/project-graph.json`) now links milestones, milestone tasks, canonical subsystems, `REQ-EV-*`, `IMP-EV-*`, `QUAL-EV-*`, E2E scenarios, decisions and documents, and carries live status.

## Why this edition exists

The architecture dossier was complete enough for human architects but could cause AI build agents to pattern-match superficially: see a familiar feature name, find a partial module, make a narrow edit and declare coverage. V3 removed that ambiguity by using native Modbit terminology, **291 native requirement rows**, canonical ownership, feature-depth contracts, mandatory existing-code audits, granular tasks and qualification tests, real-effect proof, and agent handoff/parallel-work/manifest protocols.

## Numbering scheme

| Range | Section | Contents |
|---|---|---|
| 00–09 | Authority and orientation | index, start-here, decisions, supersessions, requirement basis |
| 10–29 | Architecture and subsystems | PRD/UX, system architecture, layout, domain model, each canonical subsystem |
| 30–39 | Implementation specifications | protocol, storage, desktop, backend, observability, dependency bindings, build/buy, donor policy |
| 40–49 | Requirements, tasks and traceability | 291-row EV ledger, 265 EV tasks, 291 EV qualifications plus 20 additive EPR requirements/tasks/qualifications, roadmap, traceability, coverage gates, depth contracts |
| 50–69 | Verification and testing | test strategy, E2E catalog, security, performance, fault/chaos, conformance, real-system suites, Release Zero |
| 70–79 | Delivery and operations | CI/CD, runbook, risk register, release blockers, package integrity |
| 80–97 | Agent process and governance | anti-superficial standard, guardrails, no-placeholder gate, DoD, audit/execution/handoff/parallel protocols, templates, status vocabulary |
| 98–99 | Live state | build manifest (updated by implementation agents) |

## Read order for every agent

`../AGENTS.md` → `01_START_HERE_FOR_BUILD_AGENTS.md` → `02_AUTHORITY_AND_DECISIONS.md` → `11_SYSTEM_ARCHITECTURE.md` → `12_REPOSITORY_AND_MODULE_LAYOUT.md` → the subsystem file for the task → linked `REQ-EV-*` rows → `48_FEATURE_DEPTH_CONTRACTS.md` → linked `IMP-EV-*` / `QUAL-EV-*` / E2E → `98_BUILD_MANIFEST.md` and the project graph.

Do **not** preload the whole dossier. `89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md` governs what to load.

## File index

### 00–09 Authority and orientation
- `00_MASTER_INDEX.md` — this file
- `01_START_HERE_FOR_BUILD_AGENTS.md` — read order, product baseline, build invariant
- `02_AUTHORITY_AND_DECISIONS.md` — decision register (`MOD-*`), conflict resolution, change control
- `03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` — what older material is superseded and what survives
- `04_REQUIREMENT_BASIS_AND_LIMITS.md` — what "complete coverage" means and does not mean
- `05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md` — historical v1.0 adoption and compatibility
- `06_EPR_V1_1_SUPERSESSION_DECISION.md` — approved current supersession, delta task collision/ownership audit

### 10–29 Architecture and subsystems
- `10_PRODUCT_PRD_AND_UX.md` — product thesis, screens, journeys, acceptance
- `11_SYSTEM_ARCHITECTURE.md` — principles, deployment units, trust boundaries, data flow, failure containment
- `12_REPOSITORY_AND_MODULE_LAYOUT.md` — monorepo layout, crates, dependency direction, ownership
- `13_DOMAIN_MODEL_AND_STATE_MACHINES.md` — IDs, aggregates, state machines, event envelope, fencing
- `14_AGENT_RUNTIME_AND_ORCHESTRATION.md` — WorkGraph/AgentGraph/StateGraph, admission, capacity, steering
- `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` — provider contract, routing, failover, cache economics
- `16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` — registry, effect classes, projection, QuickJS isolate, MCP
- `17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` — normative tool families and owners
- `18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` — index stack, L0–L3 planner, Context Pack, freshness
- `19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` — seven durability layers, resume algorithm
- `20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` — file service, Git strategy, diagnostics, code surface
- `21_TERMINAL_EXECUTION_AND_SANDBOX.md` — exec contract, `modbit-execd`, sandbox boundary, guest, handoff
- `22_BROWSER_AND_COMPUTER_USE.md` — semantic browser compiler, action hierarchy, takeover, injection isolation
- `23_SECURITY_POLICY_EFFECT_LEDGER.md` — capability kernel, approvals, receipt chain, secrets, emergency stop
- `24_CLOUD_CONTROL_PLANE_AND_SYNC.md` — cloud API, worker, Postgres, object store, identity, sync
- `25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md` — MediaEnvelope, media reads, provider normalization
- `26_SKILL_REGISTRY_AND_EVOLUTION.md` — skill registry and experiment-gated Skill Evolution Lab
- `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` — adopted routing/workflow architecture and ownership

### 30–39 Implementation specifications
- `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` — SurfaceProtocol, cloud API, envelopes, event types
- `31_DATABASE_AND_STORAGE_SCHEMA.md` — SQLite/Postgres tables, object store, retention, migrations
- `32_DESKTOP_FRONTEND_IMPLEMENTATION.md` — Electron/React stack, security settings, renderer modules
- `33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` — Rust runtime composition, startup/shutdown, leases, gateway
- `34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` — tracing, metrics, logs, SLOs
- `35_DEPENDENCY_AND_BINDING_DECISIONS.md` — the only place exact dependency names are normative
- `36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` — build/buy table, fork policy, license gate
- `37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` — donor classification, extraction gate, AI reuse warning
- `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` — versioned contracts, bounded algorithms and recovery

### 40–49 Requirements, tasks and traceability
- `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` — 291 `REQ-EV-*` rows (LOCKED completeness boundary)
- `41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` — 265 `IMP-EV-*` tasks grouped by canonical owner
- `42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` — 291 `QUAL-EV-*` qualification tests
- `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` — milestones M0–M10, milestone tasks, critical path
- `44_REQUIREMENTS_TRACEABILITY_MATRIX.md` — high-level requirement → owner → crate → proof
- `45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` — trace chain and CI rule
- `46_REQUIREMENT_COVERAGE_FREEZE_GATE.md` — conditions for freezing the dossier
- `47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` — coverage audit result and its limits
- `48_FEATURE_DEPTH_CONTRACTS.md` — minimum depth per subsystem before "implemented"
- `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` — 20 additive REQ-EPR/EPR tasks with explicit prerequisites

### 50–69 Verification and testing
- `50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` — test pyramid, fixtures, evidence bundle
- `51_E2E_ACCEPTANCE_TEST_CATALOG.md` — E2E-001..025 release-gate scenarios
- `52_SECURITY_THREAT_MODEL_AND_TESTS.md` — assets, adversaries, controls, security gates
- `53_PERFORMANCE_AND_BENCHMARK_PLAN.md` — budgets, retrieval A/B/C, regression policy
- `54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md` — 30 mandatory fault cases
- `55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` — proving the tests detect broken behavior
- `56_TOOL_CAPABILITY_CONFORMANCE.md` — real-effect conformance suites per tool family
- `57_SKILL_EVOLUTION_REAL_TESTS.md` — WSK-E2E-001..010
- `58_MULTIMODAL_MEDIA_REAL_TESTS.md` — MEDIA-E2E-001..012
- `59_RELEASE_ZERO_PROOF_SCENARIO.md` — original single proof scenario
- `60_RELEASE_ZERO_EXPANDED_PROOF.md` — authoritative superset (E2E-025)
- `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` — 20 QUAL-EPR, 40 real/fault scenarios, gates A–G

### 70–79 Delivery and operations
- `70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md` — PR/nightly/RC pipelines, reproducibility, updates
- `71_OPERATIONS_RUNBOOK.md` — diagnostics, incidents, alarms, backup/restore
- `72_RISK_REGISTER_AND_OPEN_DECISIONS.md` — risks, provisional choices, go/no-go checkpoints
- `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` — non-waivable blockers
- `74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` — documentation and product CI integrity checks

### 80–97 Agent process and governance
- `80_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md` — feature-depth equation, thin-implementation traps
- `81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` — single-owner systems, forbidden edges
- `82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` — allowed doubles, CI checks, evidence record
- `83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` — universal completion checklist, specific acceptance
- `84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md` — six-step audit before touching existing code
- `85_AGENT_TASK_EXECUTION_PROTOCOL.md` — phases A–G
- `86_TASK_CARD_TEMPLATE.md` — mandatory task card
- `87_HANDOFF_AND_MANIFEST_PROTOCOL.md` — required handoff fields, forbidden handoffs
- `88_PARALLEL_AGENT_COORDINATION_RULES.md` — ownership, isolation, admission, merge
- `89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md` — minimum context bundle, retrieval rules, freshness
- `90_PR_CHANGE_EVIDENCE_TEMPLATE.md` — PR evidence sections
- `91_FEATURE_COMPLETION_AUDIT.md` — pre-release per-requirement checklist
- `92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md` — evidence classes, dependency-name confinement
- `93_STATUS_VOCABULARY_AND_LIFECYCLE.md` — the one normative status mapping
- `94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md` — historical v1.0 dossier task and evidence
- `95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md` — EPR v1.1 dossier audit, tests, revision and reseal handoff
- `96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md` — governance maintenance decision record and handoff (DOC-GOV-001): evidence grammar, decision-status validation, governing-file alignment, pinned constants
- `97_DOSSIER_MAINTENANCE_LOG.md` — append-only log of later dossier-only tasks (DOC-GOV-002 onward): decision record, handoff and evidence pointers per entry

### 98–99 Live state
- `98_BUILD_MANIFEST.md` — milestone status table updated by implementation agents; task-level status lives in the project graph

## Non-negotiable completion model

`NOT_STARTED → AUDITING → IMPLEMENTING → WIRED → REAL_TESTING → E2E_PROVEN → COMPLETE`.

No other word means done. See `93_STATUS_VOCABULARY_AND_LIFECYCLE.md` for how this maps to feature depth.
