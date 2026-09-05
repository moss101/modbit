# Execution policy router adoption decision

**Decision Record:** DR-EPR-2026-09-05  
**Status:** APPROVED for dossier adoption; production enablement remains evidence-gated.  
**Approval:** The user explicitly requested on 2026-09-05 to use the supplied patch to upgrade Modbit implementation documents, architecture, tasks, manifest and graphs. This authorizes this specification amendment; it does not attest to runtime qualification.  
**Source:** [MODBIT-PATCH-EPR-2026-09-05 v1.0](../MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md), SHA-256 `9e0cee7d49d442033b875e250a61a212b898dd6f4bf35826cdad5ffeb71edb66`.

## Trigger and current behavior

The patch expands model selection into bounded DIRECT, CASCADE and CRITIQUE workflow selection. The audited V3.1 workspace contains 70 specification documents and Python dossier tooling, with no product source or Git metadata. All product work is NOT_STARTED. Current `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` mixes task features with economics/policy in TaskFingerprint and requires model labels in Auto UI. `14_AGENT_RUNTIME_AND_ORCHESTRATION.md` specifies a single selected-model invocation loop.

The historical frontier-only reasoning and frontier-token-only efficiency rules named by the patch do not have decision IDs in this edition. Their premises are recorded as superseded in `03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`; no historical ADR ID is invented. Existing hybrid retrieval, embeddings, no mandatory local reasoning subsystem and Core ownership remain in force.

## Approved replacement and scope

Adopt ADR-R-039 through ADR-R-048 in `02_AUTHORITY_AND_DECISIONS.md`. The Model Gateway owner gains the bounded profiler/compiler and versioned registry; Core remains the executor; Policy Kernel owns risk/permission; Verification Engine owns gates; Eval Harness owns the offline Policy Lab. No new canonical subsystem or runtime is introduced.

Separate catalog-independent RequestProfile from deterministic PolicyEnvelope and versioned compiler inputs. Require complete accounting, revision-bound effect-less critique, inherited budgets, safe boundaries for re-routing, verified completion and rollback-ready evaluation. Auto model-label visibility is policy-controlled while internal auditability remains mandatory. Local Core consumes verified policy/registry bundles and stays first-class; renderer, plugins and guest workers never own routing authority or receive provider secrets.

## Migration and compatibility

The original root patch is retained unchanged as provenance. Active routing semantics are reconciled in `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`; normative algorithms and wire refinements are in `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`. Existing filename numbers and all 291 REQ-EV rows, dispositions and owner mappings remain unchanged. `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` adds REQ-EPR-000..013 and EPR-000..013, including the phase-0 baseline. `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` adds corresponding qualification, real-system, fault and release gates. This additive locked coverage extension is authorized by this record.

TaskFingerprint remains a compatibility input only: split intrinsic features into RequestProfile and external facts into the compiler/policy envelope. Existing RouterDecisionRecord can project RoutingTrace. Persist legacy Runs with no plan as explicit legacy-direct provenance, never invent historical policy versions, usage or verification. New runs use versioned plans. Major-version mismatch blocks mutation; safe export remains available. Additive event/storage migration uses the existing owners.

## Old patch destinations mapped to this edition

| Patch destination | Current authoritative destination |
|---|---|
| Historical decision/ADR register | `02_AUTHORITY_AND_DECISIONS.md`, `03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`, this record |
| 09-Agent-Runtime-and-Model-Gateway.md | `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`, `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md`, `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` |
| 12-Evidence-Evaluation-and-Robustness.md | `34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md`, `53_PERFORMANCE_AND_BENCHMARK_PLAN.md`, `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` |
| 14-Algorithms.md | `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` |
| 04-Task-Breakdown.md | `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`, `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` |
| 13-Capability-Matrix-and-Migration.md | `44_REQUIREMENTS_TRACEABILITY_MATRIX.md`, `48_FEATURE_DEPTH_CONTRACTS.md`, `37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` |

These legacy names are provenance, not missing active build files.

## Security and test impact

Model selection grants no permission. Critic tool manifests exclude mutation, shell execution, credential access and external effects; even a tool described as read-only must pass the canonical policy check. Replay uses sanitized immutable snapshots and the Gateway's service-managed inference credentials, with no production/tool credentials exposed to replay workers or model context. Policy-owned realized risk only strengthens minimum checks. Unknown-effect retries reconcile the existing ledger before any new action. Atomic workspace apply does not promise rollback of irreversible external effects.

The untouched baseline, holdout calibration, deterministic compiler rejection, complete cost accounting, actual process kill/restart, protected-path mutation tests, critic escape tests and isolated counterfactual replay are required in `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md`. Dossier checks prove traceability only. Performance and rollout thresholds must be versioned and approved before experiments; missing thresholds block promotion.

## Rollback

Before compound promotion, keep the measured direct path as the valid fallback. Production policy rollback selects the previous-good immutable policy compatible with registry/schema/data rules at a safe boundary, preserving Run identity, budget spent and effect reconciliation. Revoked models and expired registries are never restored merely because a prior policy referenced them. Rollback requires no desktop release. Reversing this specification adoption requires a new approved Decision Record; it cannot silently drop EPR coverage.

## Evidence and development entry point

Baseline hashes: `../evidence/dossier-epr/baseline.json`. Dossier task and handoff: `94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md`. Product implementation remains NOT_STARTED; start with graph-ready M0.1. EPR-000 becomes eligible only after the local provider/edit/test loop exists and is proven.
