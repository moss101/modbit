# Release Zero Execution Goal

> The executable goal that completes Modbit as a production-ready application end to end. Authorized by DR-GOV-2026-09-19-005 (DOC-GOV-005, `97_DOSSIER_MAINTENANCE_LOG.md`). This document adds no requirement, task, status or gate: it names the goal, states exactly when it is met, orders the work the graph already holds, and lists the inputs only the repository owner can supply. Progress is never recorded here; `python3 tools/graph.py goal` derives it from `../graph/project-graph.json` on every read.

## 1. The goal

**Modbit is production ready when `RELEASE_ZERO` is `READY` and the Release Zero expanded proof passes on the packaged desktop application built from a clean environment.** In the terms the dossier already defines:

| # | Condition | Defined by | Checked by |
|---|---|---|---|
| 1 | Every product work item in the graph is `COMPLETE` with qualifying evidence (78 milestone tasks and 323 implementation tasks: `IMP-EV-*`, `EPR-000..019`, adopted `PX-*`) | `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`, `83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` | `graph.py goal` (work items), `check_dossier.py` G3/G4/G9 |
| 2 | Release gates `EPR-GATE-A..G` are `SATISFIED`: every required task `COMPLETE` and the gate's own evidence attested by a release agent | `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` | `graph.py goal` (gates), `graph.py gates`, G7 |
| 3 | `M10` rolls up `COMPLETE` (conditions 1 and 2 inside M10, including the Release Zero scenario task M10.6 and the conformance harness M10.7) | `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` | `graph.py status`, G5 |
| 4 | The packaged, signed macOS candidate passes the Release Zero scenario with every fault variant and pass criterion, the RC E2E catalog, the performance gates and the feature completion audit, and no stop-the-line blocker is present | `60_RELEASE_ZERO_EXPANDED_PROOF.md`, `51_E2E_ACCEPTANCE_TEST_CATALOG.md`, `53_PERFORMANCE_AND_BENCHMARK_PLAN.md`, `91_FEATURE_COMPLETION_AUDIT.md`, `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` | the evidence of M10.3, M10.4, M10.6 and PX-031, which is what makes those items `COMPLETE` under condition 1 |
| 5 | Every live half deferred by Decision Record (DR-M2-001, DR-M3-002, DR-M3-003, DR-M6-001, DR-M6-002) has run green against the real provider or forge and is appended to its task's evidence | `../docs/decisions/`, `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` "Live provider proof", `82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` | step 1 of the Release Zero scenario authenticates to a real model gateway, so condition 4 cannot pass while a deferred half is open |

Conditions 4 and 5 are proven by tasks, so condition 1 subsumes them. The single machine check is therefore:

```bash
python3 tools/graph.py goal            # RELEASE_ZERO: exit 0 = goal met; 1 = work remains; 2 = a blocker must be resolved first
python3 tools/graph.py goal --json     # the same report for agents and CI
python3 tools/graph.py goal ALPHA      # the same derivation for an earlier release projection
```

`READY` means what `75_PHASED_RELEASE_PLAN_AND_READINESS.md` says it means and nothing more: it is derived from task status, qualification evidence and gate attestations at task level. Nobody can set it. Windows and Linux desktop promotion is decided separately by their own packaged E2E and a Decision Record (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`); it is not part of this goal.

## 2. What makes the goal executable

- **Derived, not stored.** `graph.py goal` lists every remaining step — each open work item included in the release, each unsatisfied gate, and anything they transitively wait on — layered into dependency waves: a step's prerequisites are its open `after` targets, every open work item and unsatisfied gate of each incomplete milestone its own milestone `depends_on`, and for a gate its open `requires_task` targets. Wave 0 is startable (or attestable) now. A `BLOCKED` step is a blocker; every step whose prerequisite closure contains it is reported `held by` it. The snapshot in §4 is dated; the command is the truth.
- **One lifecycle, one closing path.** Every step is a graph node. It is executed through the mandatory loop in `../AGENTS.md` and the phases of `85_AGENT_TASK_EXECUTION_PROTOCOL.md`, and it closes only through `python3 tools/graph.py set <id> <STATE>` one step at a time with `<kind>:<value>` evidence (`93_STATUS_VOCABULARY_AND_LIFECYCLE.md`). Gates close only through `python3 tools/graph.py attest`. There is no other way to make the goal's exit code 0.
- **Loop-shaped.** The intended driver is `until python3 tools/graph.py goal; do take one wave-0 step; done`, with the human decisions in §5 (credentials, threshold profiles, signing identity, attestations) taken by the owner or a release agent, never by a coding agent on its own authority.
- **Blockers are explicit.** A step that cannot be completed is set `BLOCKED --note` with its reproduction and next safe action, and any rescheduling or deferral is a Decision Record (`02_AUTHORITY_AND_DECISIONS.md`, `../docs/decisions/`). The goal command then exits 2 and names what holds the release.

## 3. State when this goal was written

Recorded on 2026-09-19 at `main` c2acfc1 (M9.3 and M9.4 sealed). Regenerate with `python3 tools/graph.py goal`.

| Milestone | Roll-up | Open work | Note |
|---|---|---|---|
| M0, M1, M2, M4, M5, M6, M7, M8 | `COMPLETE` | 0 | proven on hosted CI (macOS/Linux/Windows); rows in `98_BUILD_MANIFEST.md` |
| M3 | `BLOCKED` | 1 | `PX-020` (competence baseline) has no offline half and waits for provider credentials (DR-M3-003); every other M3 item is `COMPLETE` |
| M9 | `IN_PROGRESS` | 19 | M9.1–M9.4 `COMPLETE`; M9.5, M9.6 and 17 implementation tasks open |
| M10 | `NOT_STARTED` | 22 | depends on M3, M5, M6, M7, M8, M9; gated by EPR-GATE-A..G |

`RELEASE_ZERO`: 359/401 work items `COMPLETE`, 1 `BLOCKED`, 41 `NOT_STARTED`; gates 0/7 `SATISFIED` (A, B, C, F `TASKS_COMPLETE`; D, E, G `OPEN`); 42 open steps plus 7 attestations; one blocker (`PX-020`) holding the 22 M10 steps and gate G.

## 4. Execution stages

The stages below are the dependency order the graph encodes on 2026-09-19. `graph.py goal` prints the live wave numbers; when a stage's prerequisites change (a Decision Record reschedules a task, a blocker is resolved), the command's order is authoritative and this table is a historical snapshot. Within a stage every step is independent and may run in parallel, batched by canonical owner (`81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).

### Stage 0 — M9 backlog (startable now; M9 is unblocked)

| Step | Owner subsystem | What closes it |
|---|---|---|
| M9.5 emergency stop | effects-security | The session-wide halt already exists: `crates/policy/src/kernel.rs` refuses every non-read-only effect with `EMERGENCY_STOP` once the stop is active, leases are revoked with the reason, `EmergencyStopActivated` is journaled, and the host-owned browser stop is `IMP-EV-0085` (`COMPLETE`, M7). Proven by `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals` and `qual_ev_0085_an_emergency_stop_halts_browser_input_before_the_host_with_the_reason_on_record` in `services/modbit-core/tests/surface_protocol.rs`. The task is an audit of that path against `23_SECURITY_POLICY_EFFECT_LEDGER.md` (stop across a Core kill/restart, stop while an effect is in flight, receipts) with any gap closed in place, then a seal on the hosted run. Not a second implementation. |
| EPR-010 request/leg/gate accounting and outcomes | observability | `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` §EPR-010, QUAL-EPR-010, docs 27/34/38: every inference, reviewer, verification, retry and cache cost accounted; versions of profile/policy/registry/statistics/compiler/gate/risk logged; success, failed initial leg and escalation attributed separately |
| IMP-EV-0029, IMP-EV-0030 | model-gateway | REQ-EV-0029/0030 in `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md`, docs 15/27/38: task fingerprint and capability-based routing; specialist model chains, both on the existing ConditionalExecutionPlan (no second router) |
| IMP-EV-0040, IMP-EV-0041, IMP-EV-0066 | effects-security | REQ-EV-0040/0041/0066, docs 23/52: DevicePolicy and machine authority; policy generation with hot revalidation; reversibility and compensation classes on the Effect Ledger |
| IMP-EV-0042, IMP-EV-0139, IMP-EV-0240 | extensions-hooks (Hook Bus) | REQ-EV-0042/0139/0240: typed lifecycle hooks with timeout, scope, fail policy and audit; composable reversible registrations; a mutating hook cannot override a monotonic deny |
| IMP-EV-0137, IMP-EV-0138, IMP-EV-0183, IMP-EV-0225 | extensions-hooks (Importers, Extension System) | REQ-EV-0137/0138/0183/0225: import of other agents' config with a migration report and trust gate; unified plugins outside the trusted Core; extension context/commands/MCP resources through the importer; marketplace trust surfaced before activation, unsigned extensions quarantined |
| PX-008 | core-runtime | `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` §PX-008: a review comment from an allowed identity becomes a `TaskSteered` event with `forge_review_comment` provenance and untrusted tagging; the fake-forge half runs in CI, the real-repository half is the DR-M6-002 live half |
| PX-009 | verification | doc 62 §PX-009: real check-run results ingested as evidence artifacts with provider, run id, commit and `OutputRef` logs, shown in Review with provenance `ci` |
| Attest EPR-GATE-A, B, C, F | release agent | Their tasks are `COMPLETE`; attestation needs the gate's own evidence (§5 item 3): an approved threshold profile and measured holdout/shadow/canary/rollback results per `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` |

### Stage 1 — M9 close-out

| Step | After | What closes it |
|---|---|---|
| EPR-011 isolated counterfactual replay | EPR-010 | doc 49 §EPR-011: replay validated alternative plans offline on immutable sanitized snapshots and scratch worktrees with no production credentials or external effects; observed alternatives distinguished from estimates |
| EPR-019 independent gate calibration release suite | EPR-010 (EPR-007 and EPR-017 are `COMPLETE`) | doc 49 §EPR-019, `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`: acceptance false accept/reject, realized-risk false negative/positive and `critical_surface_miss_rate` on held-out candidates with oracle labels; unsafe gates block promotion regardless of routing gains. Gates D and E become `TASKS_COMPLETE` here |
| M9.6 security fuzz/property/attack suites | M9.5 | `52_SECURITY_THREAT_MODEL_AND_TESTS.md`, `55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md`: real attack suites against the Capability Kernel, path policy, secret broker, tenant boundary and provenance controls; each control broken once in a disposable build to prove its test fails |

M9 rolls up `COMPLETE` when its 33 work items are `COMPLETE`; update the M9 row of `98_BUILD_MANIFEST.md` in the same seal commit.

### Stage 2 — resolve the blocker (owner input)

`PX-020` is the only `BLOCKED` step and it holds every M10 step because M10 depends on M3. It resumes only when the live-provider run exists: the owner adds the repository secrets (§5 item 1), `.github/workflows/live-providers.yml` runs green, the public and internal suites run under the frozen protocol on the real M2 product with the direct configuration, and the immutable baseline bundle is recorded by digest. Then `python3 tools/graph.py set PX-020 NOT_STARTED --note "<run id>"` (a `BLOCKED` item resumes at or below the state it was blocked from) and the ladder proceeds. M3 rolls up `COMPLETE` and M10 becomes unblocked. The same run discharges the deferred live halves of EPR-000/002/005, IMP-EV-0251/0253, PX-022, PX-028 and PX-002 (DR-M3-002, DR-M3-003, DR-M6-001): append the green run to each task's `evidence.json`; those tasks stay `COMPLETE` and gain the run as evidence.

### Stage 3 — M10 foundation (after M3 and M9 are `COMPLETE`)

| Step | After | What closes it |
|---|---|---|
| M10.1 telemetry/cost/SLO dashboards | M10 unblocked | `34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md`, `71_OPERATIONS_RUNBOOK.md`: dashboards over the real usage ledger, SLO events and cost records, never a mock feed |
| IMP-EV-0017, IMP-EV-0023, IMP-EV-0032, IMP-EV-0142 | M10 unblocked | observability: dual user/model error channels; cloud SLO event ladder; per-run token/cost accounting (audit the existing `ModelUsageRecorded` → `GetTaskEconomics` path of IMP-EV-0173 first and complete it in place); trace/status/export diagnostics |
| IMP-EV-0126 headless mode | M10 unblocked | domain-events: audit the existing headless CLI (`apps/cli`, JSON lines and exit codes, attach through `core.ready`) against REQ-EV-0126 and `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` before adding anything; close only the missing links |
| IMP-EV-0211, IMP-EV-0212 | M10 unblocked | verification: multi-level tests including a real API call (needs §5 item 1); no fake test examples in the release suite, enforced by the quality gate |
| IMP-EV-0244, IMP-EV-0246 | M10 unblocked | eval-bench, disposition EXPERIMENT: task-conditioned harness generation and bounded repair of harness/profile behind the benchmark harness; never a production commitment |
| PX-030 platform CI matrix | M0.1 | governance: the macOS/Linux/Windows matrix has run since M0.1; the task records it as `CI_COMPATIBLE` only, with the conformance suites doc 62 §PX-030 names |

### Stage 4 — policy promotion and packaging

| Step | After | What closes it |
|---|---|---|
| EPR-012 joint conditional parameter evaluation and promotion | EPR-011, EPR-009, EPR-019, M10.1 | doc 49 §EPR-012, doc 38: offline search over validated plans only; confidence-adjusted feasibility and independently safe calibration before cost; controlled promotion with propensity, safe replay and previous-good rollback |
| M10.2 updater/signing/SBOM | M10.1 | `70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md`: reproducible builds, SBOM for desktop, services and guest image, artifact checksums and signatures, signed update manifests with staged rollout and rollback, migration compatibility checked before install (needs §5 item 4) |
| EPR-013 model × Skill outcome statistics | EPR-012 (M5.7 is `COMPLETE`) | doc 49 §EPR-013, `26_SKILL_REGISTRY_AND_EVOLUTION.md`: evaluation-qualified combinations in versioned Outcome Statistics, pinned in plan provenance; Skills never self-promote |

### Stage 5 — release candidate proof

| Step | After | What closes it |
|---|---|---|
| M10.3 full RC E2E catalog | M10.2, EPR-013, EPR-019 | every mandatory scenario of `51_E2E_ACCEPTANCE_TEST_CATALOG.md` on signed candidate binaries; no skipped mandatory scenario; flakes are failures until root-caused. Gate G becomes `TASKS_COMPLETE` once EPR-010/011/012/013/019 are `COMPLETE` |
| M10.4 performance regression gates | M10.3 | `53_PERFORMANCE_AND_BENCHMARK_PLAN.md`: budgets measured on reference hardware with intervals; regressions fail the candidate |
| PX-031 macOS release promotion | M10.3, PX-030 | doc 62 §PX-031, doc 76: macOS reaches `RELEASE_GRADE` by the packaged desktop E2E catalog and the applicable Release Zero subset |
| M10.5 docs/runbooks/support diagnostics | M10.4 | doc 71: diagnostics, incident and backup/restore procedures verified against the packaged build |
| PX-021 competence regression gate | PX-020, M10.4 | doc 62 §PX-021: targets set by Decision Record per metric and tier after the baseline (§5 item 3); the RC competence gate fails on regression beyond approved thresholds |
| PX-025 interaction budgets in packaged E2E | M10.4 (PX-023 is `COMPLETE`) | doc 62 §PX-025, `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`: every budget asserted with Playwright traces and Core event timestamps on reference hardware (§5 item 5) |

### Stage 6 — Release Zero and conformance

| Step | After | What closes it |
|---|---|---|
| M10.6 Release Zero scenario | M10.5 | `59_RELEASE_ZERO_PROOF_SCENARIO.md` and `60_RELEASE_ZERO_EXPANDED_PROOF.md` executed on the packaged application from a clean environment, all twenty steps, every fault variant, every pass criterion; the execution-policy release evidence of doc 60 reconstructs request → plan/leg/attempt → effects → revision-bound gate/risk evidence → cost |
| M10.7 canonical tool and capability conformance harness | M10.6 | `56_TOOL_CAPABILITY_CONFORMANCE.md`: every production tool family passes its real-substrate suite; no canned success |
| Attest EPR-GATE-D, E, G | release agent | with the measured evidence of doc 61 for each gate |

M10 then rolls up `GATED` → `COMPLETE`, `RELEASE_ZERO` derives `READY`, and `python3 tools/graph.py goal` exits 0. Update the M10 row of `98_BUILD_MANIFEST.md` and archive the release evidence bundle (`SKILLS.md` → release procedure).

## 5. Inputs only the repository owner can supply

These are not agent work. Each blocks the steps named until it exists; a coding agent records the dependency and moves to another wave-0 step rather than inventing a substitute (`82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md`).

| # | Input | How it enters | Unblocks |
|---|---|---|---|
| 1 | Provider credentials: `OPENAI_API_KEY` and `ANTHROPIC_API_KEY` as repository secrets for `.github/workflows/live-providers.yml`, and a key entered through the desktop onboarding (`PX-022`) on the packaged build | secrets in the forge; safeStorage custody on the build host; never in the repository or a prompt | PX-020 and with it M3 and M10; the deferred halves of DR-M2-001, DR-M3-002, DR-M3-003, DR-M6-001; IMP-EV-0211; step 1 of the Release Zero scenario; the measured evidence behind every EPR gate |
| 2 | Forge credentials: `MODBIT_GITHUB_TOKEN` and a dedicated test repository named by `MODBIT_GITHUB_TEST_REPO` | repository secrets per DR-M6-002 | the real-repository halves of PX-006/007/010 and the live halves of PX-008 and PX-009 |
| 3 | Approved immutable threshold profile for the execution-policy gates and competence targets: tau/delta and LCB method, sample minima, dataset/slice/lineage splits, the five error-rate limits, latency/cost tolerances, reviewer recall/churn limits, rollback triggers (doc 61 "Thresholds, priors and independent gate calibration"); competence targets per metric and tier (doc 62 §PX-021) | a Decision Record in `../docs/decisions/` with `status: accepted`; the profile pinned by digest in the gate evidence | attestation of EPR-GATE-A..G; EPR-012 promotion; PX-021 |
| 4 | Code-signing and notarization identity for the macOS candidate, the update-manifest signing key, and the SBOM/provenance tooling decision (doc 70) | signing material in the CI secret store, never in the tree; the tooling choice as a Decision Record under `36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` | M10.2, M10.3 (signed candidate binaries), M10.6, PX-031 |
| 5 | The reference hardware definition for interaction budgets and performance gates (doc 39, doc 53) | a Decision Record naming the machine class the budgets are measured on | PX-025, M10.4 |
| 6 | A staging model gateway and a sandbox host for the cloud variant of the Release Zero scenario (doc 60 step 1 and the "cloud variant loses sandbox" fault) | environment owned by the operator; the hosted CI rig (Postgres, S3-compatible store, Firecracker on a KVM runner) already exists for qualification | M10.6 |
| 7 | Optional: Windows and Linux desktop promotion decisions (doc 76) | Decision Records after their own packaged E2E | nothing in this goal; platform promotion is separate |

## 6. Per-step protocol

1. **Select.** `python3 tools/graph.py goal` and take a wave-0 step whose milestone is unblocked (`python3 tools/graph.py ready --release RELEASE_ZERO` lists the same set). `python3 tools/graph.py show <id>` gives its requirement, qualification, owner subsystem, prerequisites and scenarios. Read the task card (docs 41, 49 or 62), the requirement row, the qualification row and the owner subsystem's specifications in the order `01_START_HERE_FOR_BUILD_AGENTS.md` sets.
2. **Audit.** Classify what exists (`84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md`; `PRODUCTION-WORKING` … `NOT-FOUND`) by tracing from the production caller to the real effector or store; record the first missing link on the task card. Complete an existing implementation in place; never add a second owner.
3. **Implement and prove.** A vertical slice on a `wip/*` branch with a pull request as the hosted CI loop (macOS, Linux, Windows); real substrate for every production behavior; the qualification test named after the task (`qual_<id>`); fault injection per `54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md`; the evidence tier declared by behavioral risk (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`). Land on `main` by squash.
4. **Evidence.** The task card and `evidence.json` under `../evidence/m9/<id>/` or `../evidence/m10/<id>/` with `run:` (the green hosted run), `commit:` (the landing commit), `test:` (the qualification test), `artifact:` (retained bundles) references (`86_TASK_CARD_TEMPLATE.md`, `90_PR_CHANGE_EVIDENCE_TEMPLATE.md`).
5. **Seal.** Walk the ladder one step at a time — `python3 tools/graph.py set <id> AUDITING`, `IMPLEMENTING`, `WIRED`, `REAL_TESTING`, `E2E_PROVEN`, `COMPLETE --evidence run:<run id> --evidence commit:<sha> --evidence test:<name>` — then the five-step reseal of `74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`, the `98_BUILD_MANIFEST.md` row when a milestone rolls up, and a seal commit on `main`. Merge `main` into every open branch right after a seal; a branch that conflicts with `main` receives no CI run.
6. **Gates.** A release agent attests with `python3 tools/graph.py attest EPR-GATE-x --evidence artifact:<retained gate evidence> --agent <name>` only after the gate's tasks are `COMPLETE` and the doc 61 evidence exists. Attestation is refused earlier by the tool and rejected by G7.
7. **Stop the line.** On any item of `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md`: `python3 tools/graph.py set <id> BLOCKED --note "<blocker, reproduction, next safe action>"`, fix the root cause, never downgrade the test. Rescheduling is a Decision Record, never a silent move.
8. **Never** close a step on a mock, a disabled security check, a weakened assertion, a simulated restart, direct internal invocation instead of production routing, or a graph edit without evidence (`../AGENTS.md` "Forbidden completion shortcuts").

## 7. Maintenance

This document changes only when the goal's definition changes (a new release projection, a new gate, a rescheduled dependency by Decision Record), through a dossier task logged in `97_DOSSIER_MAINTENANCE_LOG.md`. It never changes to record progress: §3 and §4 are a dated snapshot, and `python3 tools/graph.py goal` is the live plan. `tools/test_dossier.py` exercises the command on a copied package: unknown release refused, wave-0 startable steps, milestone and gate waits, a `BLOCKED` step holding its dependents with exit 2, release scoping, and exit 0 once every work item is `COMPLETE` and every gate attested.
