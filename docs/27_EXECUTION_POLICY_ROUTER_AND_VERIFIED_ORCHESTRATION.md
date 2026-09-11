# Execution policy router and verified orchestration

> **Authority date:** 2026-09-05; **edition:** V3.3 EPR v1.1.  
> **Adopted by:** `06_EPR_V1_1_SUPERSESSION_DECISION.md`; ADR-R-039..056 with explicit v1.1 amendments.  
> **Implementation:** NOT_STARTED. This is the active architecture specification, not evidence of a working router.

This active specification preserves non-conflicting v1.0 profiling, accounting, context, skills, recovery, security and evaluation rules while replacing the clauses mapped in doc 06. Existing section numbers remain stable for provenance. Both root patches are historical source artifacts; this numbered document and doc 38 define the current executable contracts. Task/qualification updates are in docs 49/61.

## Canonical ownership and local execution

| Responsibility | Existing owner | Implementation location |
|---|---|---|
| Request profiling, registry and deterministic plan compiler | model-gateway | `crates/providers` routing modules |
| Bounded leg execution, budgets and routing epoch | core-runtime | `crates/core-runtime` |
| Identity/risk/permission envelope and effect authorization | effects-security | `crates/policy`, `crates/effects`, `crates/secrets` |
| Run/event/protocol persistence | domain-events | existing domain, event-store and protocol-state crates |
| Candidate isolation/apply and recovery | workspace-git, durability | existing worktree/change engine and checkpoints |
| Quality gates and factual verification | verification | `crates/verification` |
| Policy Lab, Outcome Statistics and counterfactual evaluation | eval-bench | existing benchmark/eval harness and event/artifact storage |
| Per-leg context and skill compilation | context-engine, skills | existing context and prompt/skill compiler |
| Outcome and usage projections | observability | existing cost/observability owner |

Server/control-plane authority means privileged policy publication and Core-side enforcement. Local Core may compile and execute from a verified, fresh, compatible policy/registry bundle; it does not require a cloud-only brain or a general LLM routing call. Hosted workers use the same contracts. The renderer carries user preferences and projections only. A locally configured compatible endpoint is eligible only through the same Gateway, admission and evaluation rules; no mandatory local SLM is added.

The profiler's `p_floor_success` is calibrated to a versioned qualification reference cohort, not a hidden live model name. The feature representation remains catalog-independent; registry generation and model/Skill slice estimates are joined with versioned Outcome Statistics by the compiler, and calibration drift is rechecked on registry changes. Example probabilities below are illustrative, never production thresholds.

## 1. Purpose

Modbit compiles the cheapest policy-compliant bounded conditional transaction whose confidence-adjusted verified quality clears the selected floor. The existing Core executes an initial solver leg, gathers deterministic/static evidence, derives factual assurance requirements and activates only prevalidated continuation slots when acceptance requires stronger solving, independent review or human intervention.

```text
request -> intrinsic RequestProfile + canonical PolicyEnvelope
        -> Conditional Plan Compiler
           + Model Registry + Skill Registry + Outcome Statistics
           + cache/session state + current economics/health
        -> initial solver -> static evidence -> RealizedRisk
        -> Acceptance Gate -> ACCEPT / prevalidated escalation / review / human
        -> exact request, leg and gate outcomes -> offline Policy Lab
```

The runtime has one orchestration core. The router does not generate an arbitrary agent graph, own another Run/Session store, or decide authorization. Evidence-first verification, worktree/checkpoint safety, inherited budgets, complete accounting and controlled rollout remain canonical.

## 2. Authority amendments

### 2.1 Superseded rule: frontier model as sole production reasoning intelligence

The previous rule:

> The frontier LLM is the sole production reasoning intelligence.

is superseded.

Replace it with:

> **Modbit may use multiple eligible reasoning models through a governed Execution Policy Router. A frontier model remains the quality backstop, not a mandatory first-hop for every request. No model may bypass Modbit policy, capability, context, write-safety, verification, credential, or effect boundaries.**

This amendment does **not** require a local small model. The economical floor, frontier solver, reviewer or specialist may be hosted, private, custom-compatible or otherwise available through the Model Gateway.

The existing rule that a mandatory local SLM is not a production dependency remains unchanged.

### 2.2 Superseded rule: frontier tokens as the primary efficiency metric

Replace:

> frontier-model tokens per verified successful outcome

with:

> **total inference cost + human intervention + wall-clock per verified successful outcome, subject to a required quality floor.**

Also report model-token detail, cache cost, retries, escalation, reviewer cost, deterministic verification cost, and the estimated or observed counterfactual cost of a direct frontier execution.

### 2.3 Refined rule: conditional execution policy

Adaptive routing selects initial solver, evidence profile, assurance/acceptance parameters and finite continuation slots. ConditionalExecutionPlan is the sole executable planning object. DIRECT/CASCADE/CRITIQUE are derived executed-path labels, never primary templates or dispatch selectors. New slot/parameter semantics require benchmark/holdout and controlled rollout evidence before promotion; no runtime-generated topology is permitted.

## 3. Product outcome

The architecture is intended to provide five concrete benefits.

### 3.1 Absorb routing mistakes inside the system

A pure model router must be correct before execution. If it under-routes, the user experiences a weak result.

A verified execution policy can instead:

```text
economical model
      |
      v
candidate result
      |
      v
cheap quality gate
   /       \
PASS       FAIL
 |           |
done      escalate
             |
             v
        frontier solver
```

The initial predictor therefore does not need to be perfect. The runtime can detect many weak drafts before exposing completion to the user.

### 3.2 Exploit the uncertain middle

Many requests are neither obviously trivial nor obviously frontier-only.

For those requests, Modbit can attempt an economical configuration first and pay frontier cost only when verification rejects the economical attempt.

### 3.3 Use complementary model failure modes

For high-risk or high-value work, Modbit can use:

```text
solver A -> independent reviewer B -> solver A revision
```

The reviewer is selected for defect-detection value and diversity of failure mode, not merely raw coding benchmark strength.

### 3.4 Make economical models more useful

An economical model does not need to match the strongest frontier model on every task.

It needs to solve a sufficiently large portion of requests cheaply, while Modbit verification and selective escalation protect final verified quality.

Runtime Skills may increase the economical model's task-specific success rate, but Skills remain instruction packages and never grant permissions.

### 3.5 Optimize the entire verified task

The system optimizes complete request economics:

- initial model invocation;
- cached and uncached input;
- output;
- critique;
- revision;
- escalation;
- retry;
- fallback;
- verification;
- latency;
- human intervention;
- recovery.

A local token-saving optimization that increases retries or user correction is a regression.

---

## 4. Architectural invariants

The following are mandatory.

1. **No router LLM on every request.** The hot-path profiler must be a small learned or deterministic model with a strict latency budget.
2. **Model-independent task representation.** The learned request profile must not depend on current model names, provider prices, cache residency or organization allowlists.
3. **Server/control-plane routing authority.** Routing policy, capability profiles, model economics and policy weights are not embedded as desktop business logic. The client may display routing state and execute local tools but does not own the routing policy.
4. **One execution core.** ConditionalExecutionPlan executes through existing Run/Session/RunSteps; DIRECT/CASCADE/CRITIQUE classify the path afterward.
5. **No arbitrary generated workflow DAGs in the initial architecture.**
6. **No unbounded model debates, committees, recursive reviewers or best-of-N tournaments by default.**
7. **Every leg is bounded.** Every model leg has a token/output budget, timeout, cancellation signal and retry ceiling.
8. **Budget inheritance.** Escalation receives the remaining request budget, not a fresh unlimited budget.
9. **Isolated Non-Committing Reviewer.** Canonical tracked state is read-only; bounded sandbox execution and ephemeral writes are allowed only in a disposable review environment. No canonical mutation, Git commit/push, deploy or persistent/external actions. Network/secrets deny by default.
10. **Verification is evidence-first.** Deterministic checks are preferred to LLM judging whenever a reliable deterministic oracle exists.
11. **Autonomous mutation uses isolated worktree/checkpoint semantics.**
12. **No completion based on model confidence alone.**
13. **Risk is policy-owned.** A learned scalar never grants permission or weakens a required review/verification path.
14. **Routing is cache-aware.** An alternative must remain confidence-feasible and cheaper after lost cache, refill, cache write, latency and hysteresis costs.
15. **Routing changes are evaluation-gated and versioned.**
16. **Hosted provider credentials remain behind the Model Gateway boundary.**
17. **Manual model selection may remain available outside Auto mode when product/org policy permits it. Auto mode exposes objectives, not a requirement to pick model names.**
18. **Routing telemetry never becomes authorization authority.**
19. **Routing failure fails safe to a validated fallback plan or explicit error; it never silently executes an invalid plan.**
20. **No fixed model name is normative architecture. Models occupy roles.**
21. **Prevalidated slots only.** Every reachable continuation is policy-checked and worst-case budgeted before the initial leg; runtime only activates compiled slots.
22. **Confidence feasibility first.** Means alone cannot qualify cheap plans; label QUALITY_FLOOR_INFEASIBLE when no hard-eligible plan clears the conservative quality criterion.
23. **Separate statistics and gates.** Empirical outcomes are independently versioned; factual risk sets assurance and Acceptance Gate checks evidence. Gate safety is release-critical independent of router gains.

---

## 5. Runtime architecture

### 5.1 Policy Envelope

Derive the envelope from canonical identity, organization, repository, mode and task state before compiling or dispatching. RequestProfile cannot alter it. It defines legal models/providers/plans, permitted effect classes, protected surfaces, minimum assurance, mandatory review/human conditions, secret/network/deploy rules and request/daily budgets. A generic learned risk scalar is not required.

```yaml
PolicyEnvelope:
  schema_version: string
  policy_version: string
  tenant_id: string
  user_id: string
  repository_id: string
  run_id: string
  allowed_providers: []
  allowed_models: []
  blocked_models: []
  data_residency: optional
  retention_class: optional
  cost_cap: {request_max: money, daily_remaining: optional_money}
  latency_policy: {deadline_ms: optional_integer}
  objective_mode: COST | BALANCE | INTELLIGENCE | CUSTOM
  quality_floor: probability
  confidence_requirement: {delta: probability, method_ref: string}
  permission_mode: supervised | autonomous | governed
  minimum_assurance: FAST | STANDARD | GOVERNED | HIGH_ASSURANCE
  protected_surfaces: []
  required_checks: []
  mandatory_review_conditions: []
  mandatory_human_conditions: []
  permitted_effects: []
  forbidden_effects: []
  network_secret_deploy_policy_ref: string
```

Any legacy risk_prior is policy-derived provenance for these minima, never a learned permission or acceptance score. Neither a mode nor manual pin weakens the envelope.

## 6. Layer 1 - Request Profiler

### 6.1 Mission

Produce a compact, catalog-independent representation of what the request appears to require.

The profiler predicts the **task**, not the current economics of available models.

### 6.2 Output contract

```yaml
RequestProfile:
  version: string

  task_type:
    one_of:
      - explain
      - edit
      - debug
      - refactor
      - test
      - greenfield
      - review
      - investigate
      - migrate
      - operate
      - mixed

  domain:
    primary: string
    secondary: []
    confidence: 0.0..1.0

  modifiers:
    - cross_file
    - cross_package
    - unfamiliar_framework
    - visual
    - security_sensitive
    - long_horizon
    - sparse_tests
    - repository_wide
    - external_system
    - other

  capability_demand:
    reasoning: 0.0..1.0
    code_generation: 0.0..1.0
    debugging: 0.0..1.0
    tool_horizon: 0.0..1.0
    context_pressure: 0.0..1.0

  p_floor_success: 0.0..1.0
  prediction_confidence: 0.0..1.0
  ood_score: 0.0..1.0

  feature_digest: string
```

### 6.3 Inputs

Allowed hot-path inputs include:

- original request;
- task mode;
- explicitly selected scope;
- repository language/framework summary;
- repository size bucket;
- changed-file and current-diff features;
- number of relevant files/symbols;
- test presence;
- diagnostics presence;
- build-system type;
- task history features that describe task state;
- turn index;
- coarse context-pressure features;
- prior execution outcome features from the same Run when re-profiling is explicitly permitted.

### 6.4 Inputs that must not contaminate the learned task representation

Do **not** feed these as intrinsic task features:

- model name;
- provider name;
- current model price;
- model availability;
- model degradation status;
- current cache residency by model;
- organization model allow/block state;
- Cost/Balance/Intelligence preference.

Those belong to Layer 2.

### 6.5 Initial implementation

The production hot path must not require a general LLM classification call.

Permitted initial implementations:

- small fine-tuned encoder;
- embedding + gradient-boosted model;
- calibrated linear/tree ensemble;
- deterministic classifier plus learned `p_floor_success`;
- hybrid of the above.

Target:

- p50 under 25 ms where practical;
- p95 under 50 ms on the routing service;
- bounded feature extraction;
- no repository-wide scan on the routing critical path.

### 6.6 Calibration requirements

The profiler must be evaluated for:

- Brier score / probability calibration;
- expected calibration error;
- ROC/PR where meaningful;
- accuracy by task/domain/modifier;
- OOD detection;
- floor-success calibration;
- drift by model-registry generation;
- drift by repository language/framework;
- drift by product version.

A high aggregate score does not excuse poor calibration on high-risk categories.

---

## 7. Layer 2 — Conditional Plan Compiler

### 7.1 Versioned inputs

Join RequestProfile, PolicyEnvelope, Model Registry, Skill Registry, separately versioned Outcome Statistics, cache/session state, current price/health/latency, budgets and available verification. Freeze all input identities for deterministic replay. Profiler intrinsic features never contain model names, price, availability, organization allowlists, objective preference or cache state.

### 7.2 Model Registry

The registry contains model/config identity/revision, provider/family/roles, capabilities, context/output limits, tool/schema compatibility, supported vision/media, current economics, latency, health/availability and governance. Registry signatures, generation, freshness and compatibility are required. It is configuration, not empirical workflow history.

```yaml
ModelProfile:
  model_id: string
  revision: string
  provider_id: string
  family: string
  roles: [ECONOMICAL_FLOOR, FRONTIER_SOLVER, REVIEWER, SPECIALIST]
  capabilities: {}
  context_limits: {}
  output_limits: {}
  tool_compatibility: {}
  vision_capability: optional
  current_cost: {input: money, output: money, cache_read: optional_money, cache_write: optional_money}
  latency: {p50_ms: integer, p95_ms: integer}
  health: {}
  availability: healthy | degraded | unavailable
  governance: {allowed_tenants: [], blocked_tenants: [], data_policy_tags: []}
```

### 7.3 Outcome Statistics and role qualification

Outcome Statistics is a separately versioned derived dataset owned by existing eval-bench and stored through existing event/artifact/storage owners. It is not another memory, event or protocol authority. No new mandatory database or service is introduced. Its consumer interface can provide immutable snapshots locally and in hosted Core.

```yaml
OutcomeStatistics:
  stats_version: string
  source_outcome_versions: []
  data_partition_digest: string
  calibration_method_version: string
  solver_success:
    key: [demand_bin, model_config, skill_set, harness_version]
    value: [mean, confidence_interval, sample_count]
  escalation_probability:
    key: [demand_bin, solver_config, skill_set, gate_version, repository_slice, verification_availability]
    value: [probability, confidence_interval, sample_count]
  critic_value:
    key: [solver_family, reviewer_config, task_slice]
    value: [defect_recall, false_positive_rate, critical_defect_recall, confidence_interval, sample_count]
  revision_success:
    key: [solver_config, reviewer_config, finding_class]
    value: [probability, confidence_interval, sample_count]
```

The historical `critic_value` statistic name remains an explicitly compatible data key; the executing role is REVIEWER. Store solver-relative failure correlation, security/API/regression recall, tool reliability and cost/latency observations with versions/sample counts as applicable. Escalation is a solver/task/Skill/gate/repository/verification interaction, not an intrinsic ModelProfile property. Missing or stale statistics lower confidence; never fabricate samples or assume a benchmark mean is reliable product evidence.

An economical floor must satisfy tool/context/format/health eligibility and confidence-adjusted product evidence on its slice. If none qualifies, remove cheap initial/continuation candidates. Frontier remains a quality backstop, subject to the same hard policy and budgets. Reviewer selection uses relational defect detection and independent failure modes, not solver prestige. Skill/effort/context configurations change qualified statistics, not intrinsic task features or capability grants.

### 7.4 Candidate enumeration

Enumerate finite ConditionalExecutionPlans with one initial solver, evidence profile, realized-risk and acceptance policies, and optional quality-reject escalation, review/revision, human-wait and provider-fallback slots. Only offline-qualified parameter/slot semantics are eligible. No arbitrary generated DAGs, recursive debates or runtime branch synthesis. Reject policy, residency, modality, role, manual-pin, context, availability, assurance, isolation or budget violations before quality/cost selection.

### 7.5 Confidence-adjusted feasibility and objective

```text
minimize E[total_cost(plan)]
subject to:
  LCB(Q(plan | request, versioned_statistics)) >= quality_floor(mode)
  # or an approved equivalent P(Q >= tau) >= 1 - delta
  worst_case_cost(plan) <= remaining_request_and_daily_budget
  policy(plan) == allowed
  required_assurance(plan) is satisfiable
  context, availability, isolation and latency constraints hold
```

Modes set quality floors and confidence requirements, not merely cost weights. COST, BALANCE (default), INTELLIGENCE and organization CUSTOM profiles remain available; all maintain policy minima. Among feasible plans, choose lowest expected total cost; use declared latency/intervention/recovery comparisons and a stable digest tie-break without buying permission to violate quality. Preserve separate units or pin any conversion factors.

Published/internal benchmarks seed low-confidence priors until representative product evidence exists. A high mean with weak LCB does not qualify a cheap plan. If the quality-feasible set is empty, choose the highest-confidence/highest-quality **hard-eligible** plan within budget and policy, record QUALITY_FLOOR_INFEASIBLE and disclose that the target was not met. Pin its ranking/tie-break rule and estimates. If the hard-eligible set is also empty, fail explicitly. Neither branch waives assurance, acceptance, isolation, budget or approval. Prediction of feasibility is never evidence of task completion.

### 7.6 Cache economics and routing epochs

```text
switch_cost = lost_cache_value + re_prefill_cost + cache_write_cost
            + expected_switch_latency + hysteresis_penalty
```

Account for cached/uncached input, cache TTL/read/write pricing, lost stable-prefix reuse, output and context reconstruction. Compare cost on the same accounting basis without double counting cache loss/refill; latency/hysteresis units and conversion policy must be explicit. Switch only when a confidence-feasible alternative remains better after that cost. Preferred re-evaluation boundaries: new task/request, compaction/context epoch, significant demand drift, provider degradation, quality rejection, task-mode or explicit objective change.

A boundary is an opportunity to reevaluate, not authority to synthesize a branch. Within an active transaction, activate only its prevalidated slot. If a different topology is necessary, stop/reconcile it first, then compile a distinct transaction using remaining request budget and a new fenced epoch.

As built (EPR-009, the offline half): `crates/providers::economics` prices staying against switching on one basis — the same single leg at the expected prompt size and output ceiling the compiler prices a plan's initial slot with — with the registry's cached-input, cache-write and cache-TTL prices (`Economics`, optional, absent = no discount, free write, the provider default TTL), the session's warm prefix as the provider last reported it (`prompt_tokens_details.cached_tokens`), the added p50 latency priced at an explicit rate and a hysteresis margin in basis points of the stay cost; the re-prefill is counted once (inside the alternative's full price) and the lost cache value is reported, never added. The lowest switch cost over the alternatives is the threshold the compiler's selection applies to the incumbent (`current_binding`; a fresh compile without one digests as before). With nothing confidence-feasible the plan in force stays in force. The Core re-evaluates at the task boundary (the session's route and warm prefix as the incumbent's asset) and at every compaction epoch (the prefix is gone; cold economics); a switch compiles, admits and activates a new transaction under the next routing epoch on the same run, and a resumed run continues on the plan of the highest epoch. Every decision is `RouteReevaluated` on the run (boundary, epoch, current, chosen, STAY | SWITCH | INITIAL, both totals, the itemized switch cost, the cache state consulted, the plan in force), and `RoutingSessionState` (§16.1) is served as a projection of the log (`GetRoutingSessionState`, CLI `session route`). Provider, quality and mode boundaries wait for their producers (EPR-006/007).

### 7.7 Compile-time validation

Validate all initial and continuation model bindings/revisions, role and policy eligibility, context/verification compatibility, enforceable reviewer isolation, data rules, finite retries/timeouts/revisions, terminating fallback chains and worst-case total budget before execution. Reserve mandatory checks plus reviewer tool/process costs and every reachable sequential continuation, including review then escalation if allowed. Validate mutually exclusive alternatives as alternatives but never under-reserve paths that can occur together. Persist validation digest, input versions, slot table and reservation atomically. Runtime rechecks revocation/lease/revision; it cannot expand the topology or capabilities.

## 8. ConditionalExecutionPlan contract

This is the sole canonical executable plan. Example YAML is refined by `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`.

```yaml
ConditionalExecutionPlan:
  schema_version: 2
  plan_id: uuid
  transaction_id: uuid
  run_id: uuid
  routing_epoch: integer
  policy_version: string
  profiler_version: string
  compiler_version: string
  model_registry_version: string
  skill_registry_version: string
  outcome_statistics_version: string
  gate_version: string
  realized_risk_version: string
  request_profile_ref: string
  policy_envelope_ref: string
  quality_prediction: {mean: probability, lcb: probability, method_ref: string}
  quality_feasibility: FEASIBLE | QUALITY_FLOOR_INFEASIBLE
  initial_leg:
    leg_id: string
    role: solver
    model_config_ref: string
    skill_refs: []
    context_policy_ref: string
    tool_capability_ref: string
    max_input_tokens: integer
    max_output_tokens: integer
    timeout_ms: integer
    max_retries: integer
  evidence_profile_ref: string
  realized_risk_policy_ref: string
  acceptance_requirements_ref: string
  continuations:
    QUALITY_REJECT: optional_prevalidated_escalation_slot
    REVIEW_REQUIRED: optional_prevalidated_reviewer_revision_slot
    HUMAN_REQUIRED: optional_prevalidated_human_wait_slot
    PROVIDER_FAILURE: optional_prevalidated_fallback_slot
  on_accept: atomic_commit
  max_total_attempts: integer
  max_revisions: integer
  workspace_policy: {isolated_worktree: boolean, checkpoint_before: boolean, atomic_apply: boolean}
  budget: {request_cost_cap: money, reserved_worst_case: money, remaining_at_compile: money}
  compiler_reason: {primary_factors: [], excluded_plans: []}
```

Every slot has stable ID, allowed predecessor/trigger, bounded leg references and activation count, model/tool/context/timeout/retry caps, terminal behavior and reservation share. Reviewer slots additionally bind disposable environment, process/network/secret/path ceilings and cleanup. Human-wait slots bind the existing approval service, required scope and finite wait/cancellation policy. Acceptance is runtime-mediated atomic workspace apply; it does not automatically grant Git commit or external effects.

DIRECT labels an initial-leg-to-accept path; CASCADE labels an executed escalation; CRITIQUE labels an executed review/revision. Preserve ordered path records when multiple labels apply. Labels never select executable topology. Retain original request and complete provenance; old ExecutionPlan decodes only through an explicit validated migration.

## 9. Layer 3 — Bounded Transaction Executor

### 9.1 One executor and inherited bounds

Core executes the initial leg and admitted slots through existing Run/Session, AgentGraph/RunSteps, worktree/checkpoints, Gateway, tools, policy, approvals, verification, effects and recovery. Every leg receives input/output/token/process/tool budgets, wall timeout, cancellation, finite retry ceiling and permitted capabilities. Persist attempt and reservation before dispatch. A continuation receives remaining budget, never a new full request allowance.

```text
escalation maximum <= request budget - initial spend - evidence/review spend
                      - in-flight reservations - reserved mandatory verification
```

Apply the same invariant to reviewer/reviser/provider fallback and repeated evidence. Include bounded reviewer tool execution in pricing. Normal solver tools remain read/search/edit/terminal/build/test/browser or other registered capabilities allowed by task policy; external effects still require ordinary protected-effect governance.

### 9.2 Evidence-first ordering

Initial/revised candidate → deterministic/static evidence → factual RealizedRisk → Acceptance Gate. Use builds/typecheck/lint, targeted/regression tests, acceptance/scope/file-touch checks, API/dependency/lockfile/migration/diagnostic/security deltas and tool-failure/incomplete-work evidence. Do not call a reviewer merely to rediscover known compiler/type/test failures.

If evidence suffices, accept. If solver quality is inadequate, select the prevalidated escalation slot. If independent review is required, select the reviewer/revision slot. If policy requires human action, use its precompiled human slot and existing approval state. Multiple obligations remain pending until each is satisfied; activation order and all reachable combinations are compiled and budgeted. Missing required slot produces explicit attention/failure after safe reconciliation, never an invented branch or unverified accept.

### 9.3 Factual realized risk

RealizedRisk answers how much assurance the candidate requires. It is deterministic/policy-owned and revision-bound, separate from correctness acceptance.

```yaml
RealizedRisk:
  schema_version: string
  realized_risk_version: string
  candidate_revision: string
  level: LOW | MEDIUM | HIGH | CRITICAL
  reasons: []
  minimum_assurance: FAST | STANDARD | GOVERNED | HIGH_ASSURANCE
  independent_review_required: boolean
  human_required: boolean
  evidence_refs: []
```

Reasons cover protected/auth/authorization/secret surfaces, dependencies/lockfiles, CI/CD/infrastructure, migrations, public API, blast radius, sparse coverage, unexpected scope and external-effect requests. Post-draft facts may strengthen assurance but cannot weaken PolicyEnvelope minima. A generic learned risk classifier is not an authorization source.

### 9.4 Acceptance Gate

Acceptance answers whether current evidence satisfies required assurance; it is not the risk classifier.

```yaml
AcceptanceGateResult:
  schema_version: string
  gate_version: string
  plan_id: string
  leg_id: string
  candidate_revision: string
  realized_risk_ref: string
  required_assurance: string
  evidence:
    build: optional
    typecheck: optional
    lint: optional
    tests: optional
    security: optional
    api_compatibility: optional
    reviewer: optional
  verdict: ACCEPT | REJECT | INCONCLUSIVE
  missing_evidence: []
  evidence_refs: []
```

Missing, stale, cancelled or timed-out mandatory evidence cannot ACCEPT. A semantic judge is allowed only where deterministic evidence is insufficient and measured evaluation justifies it; it cannot waive hard checks or gain effect authority. Runtime maps the typed gate result plus pending assurance obligations to a compiled slot. Gate/risk false-accept/false-reject/miss metrics are independently release-critical.

### 9.5 Isolated Non-Committing Reviewer

Canonical tracked state is READ ONLY. A throwaway review worktree may receive EPHEMERAL WRITES and bounded SANDBOXED process execution for evidence gathering. Network and secrets DENY BY DEFAULT. Canonical workspace mutation, Git commit/push, deployment, external actions and persistent effects are DENIED. Tool permissions bind actual paths, mounts, process tree, network/secret handles and environment lifetime; a tool labeled read-only is not a security proof.

The reviewer receives original task, acceptance criteria, candidate diff, relevant scoped source/context, static verification, diagnostics/targeted tests and dependency/API/security evidence. It SHOULD NOT receive solver hidden reasoning; independence is separation from solver reasoning, not ignorance of source code. The default compiler excludes hidden-reasoning channels. Gateway credentials remain Gateway-owned; none is exposed to reviewer tools/context.

Reviewer findings are structured, untrusted until validated and exact-revision-bound. ReviewerResult carries verdict pass/revise/block/uncertain, confidence, finding IDs, severity/category, claim/evidence refs, suggested action and unresolved questions. Unsupported findings remain unresolved assertions, never permission or verified fixes. Revised candidates invalidate stale review; required re-review must already fit the slot table and bound. Canonical writes occur only through the normal solver/reviser and accepted change engine, never by applying the reviewer's scratch tree.

On cancellation/timeout/crash, stop the bounded process tree, revoke handles and dispose scratch state. Trusted Core may extract bounded content-addressed evidence through the existing artifact/evidence owner before cleanup; this grants no reviewer persistent-write capability. Actual isolation/cleanup proof is required before review admission; an unavailable supported sandbox means the slot is ineligible.

### 9.6 Atomic acceptance and recovery

Autonomous work uses existing isolated worktree/checkpoint semantics. Accept only after current-revision assurance/evidence, required workflow legs and critical findings resolve, budget/accounting obligations hold and compare-and-apply workspace preconditions pass. Failed/cancelled/blocked transactions leave no partially accepted patch. A model may propose completion; Core decides acceptance. Unresolved exceptions are surfaced, never labeled verified success.

Irreversible external effects still use the ordinary Effect Ledger and are not undone by worktree rollback. Persist plan/slot activation/leg/attempt, revisions, gate/risk/reviewer refs, accounting and epoch before transitions. Restart reconstructs exact state and reconciles pending tools/effects/charges. Stale responses cannot activate a slot twice or apply a patch. No topology is synthesized during recovery.

## 10. Routing decision and execution telemetry

Every request retains an exact RoutingDecisionRecord plus RoutingTrace usage detail. The record includes request/profile/envelope refs and versions; model and Skill registry versions; outcome_statistics_version; compiler_version, gate_version and realized_risk_version; all candidate plan IDs, quality mean/LCB, expected cost/latency, eligibility and exclusion reasons; chosen plan and ordered executed_path (slot/leg/role/outcome); final outcome and QUALITY_FLOOR_INFEASIBLE when applicable. Preserve prediction versus observation and requested versus resolved bindings. Derived DIRECT/CASCADE/CRITIQUE labels are supplementary, not dispatch authority.



```yaml
RoutingTrace:
  request_id: uuid
  run_id: uuid

  policy_version: string
  profiler_version: string
  model_registry_version: string
  outcome_statistics_version: string
  gate_version: string
  realized_risk_version: string

  request_profile_ref: string
  policy_envelope_ref: string

  eligible_plan_ids: []
  chosen_plan_id: string
  choice_probability: optional float

  routing_latency_ms: integer
  compiler_reason: []

  legs:
    - leg_id: string
      role: string
      model_config_ref: string
      started_at: timestamp
      completed_at: timestamp
      input_tokens: integer
      cached_input_tokens: integer
      output_tokens: integer
      inference_cost: money
      latency_ms: integer
      retries: integer
      outcome: success | failure | cancelled | escalated

  verification_cost: money
  total_inference_cost: money
  total_wall_clock_ms: integer

  estimated_direct_frontier_counterfactual_cost: optional money
  observed_counterfactual_cost: optional money

  final_verified_outcome: pass | fail | partial | cancelled
```

### 10.1 Complete accounting

The request cost must include all model legs.

Do not report only the final or winning model.

### 10.2 Counterfactual labeling

If the frontier path was not actually executed, report:

> estimated counterfactual

If a shadow/replay/A-B arm actually executed the alternative, report:

> observed counterfactual

Never present an estimate as observed savings.

---

## 11. Outcome and reward model

### 11.1 Reward signals

Build a composite reward from:

### Explicit
- thumbs/ratings;
- user-reported failure;
- explicit accept/reject.

### Implicit
- edit kept;
- edit reverted within a configured window;
- immediate corrective follow-up;
- user manually switches model after poor result;
- repeated repair turns;
- intervention during autonomous run.

### Hard outcome
- required tests pass;
- build succeeds;
- deterministic acceptance passes;
- security finding resolved and verified;
- PR accepted/merged where available;
- no post-completion rollback attributable to the change.

### 11.2 Reward record

```yaml
OutcomeRecord:
  request_id: uuid
  chosen_plan_id: string
  choice_probability: optional float

  verified_success: boolean
  first_pass_success: boolean

  explicit_feedback: optional
  keep_rate_signal: optional
  revert_signal: optional
  correction_turn_signal: optional

  tests_passed: optional boolean
  build_passed: optional boolean
  merge_signal: optional boolean

  human_intervention_count: integer
  repair_turns: integer

  total_cost: money
  wall_clock_ms: integer

  reward_version: string
  composite_reward: float
```

Keep raw signals. Do not retain only the composite scalar.

---

### 11.3 Request, leg and gate attribution

Retain request final verified success, satisfaction, keep/revert, intervention, cost, wall-clock and repair turns. Separately retain initial-solver accept/reject, escalation success, reviewer genuine-defect discovery/false positive, revision success and tool reliability. Gate-level records retain accepted results later shown wrong, rejected results later shown valid, realized-risk misses and over-classification. A successful frontier escalation MUST NOT credit the failed initial economical leg. Preserve linkage to exact candidate, gate/risk versions and later corrective evidence; do not retain only a composite reward scalar.

## 12. Counterfactual learning

### 12.1 Why it is required

A router trained only on its own historical choices suffers selection bias.

If the old policy sends easy work to the floor model and hard work to frontier models, the dataset cannot directly reveal how either model would have performed on the other slice.

### 12.2 Exploration

A small configurable fraction of eligible requests may be assigned an alternative validated plan for learning.

Production exploration must be:

- policy allowed;
- budget bounded;
- excluded from critical/high-assurance tasks unless explicitly approved;
- capability safe.

### 12.3 Shadow and replay isolation

Alternative plans must never duplicate real external effects.

Preferred methods:

1. immutable repository snapshot + scratch worktree;
2. no credentials;
3. no external side effects;
4. replay recorded deterministic tool evidence where valid;
5. offline/asynchronous counterfactual execution;
6. explicit test environments for effectful scenarios.

### 12.4 Propensity logging

When policy exploration is probabilistic, log `choice_probability`.

This enables off-policy evaluation and reduces self-confirming router bias.

---

## 13. Offline Policy Lab

The policy-learning/evolution system is **not** on the request critical path.

### 13.1 Inputs

- versioned benchmark corpus;
- production trajectories;
- RequestProfile outputs;
- candidate model profiles;
- costs;
- latency;
- cache behavior;
- verification outcomes;
- reward signals;
- failure modes;
- policy constraints.

### 13.2 Conditional parameter search

Search initial solver/effort, Skill set, evidence profile, acceptance and realized-risk thresholds, escalation solver, reviewer and reviewer tool budget, reviser, continuation thresholds, token/context tiers, timeouts and retry ceilings. Tune these jointly offline using pinned Outcome Statistics. The legal shape is a bounded conditional transaction; never search arbitrary graphs or create new branches during execution.

### 13.3 Search objective

Stage A reaches confidence-adjusted verified-quality feasibility and hard safety constraints. Stage B minimizes complete cost, latency, intervention and recovery burden along the feasible boundary. Bounded configuration-search algorithms remain replaceable; empirical confidence, untouched holdout, independent gate safety and controlled promotion are not. High means from published benchmarks stay low-confidence priors until representative evidence qualifies them.

### 13.4 Promotion pipeline

```text
training / replay
      |
      v
validation
      |
      v
untouched holdout
      |
      v
shadow
      |
      v
small canary / A-B
      |
      v
production
      |
      v
continuous regression monitoring
```

A new predictor, model-profile generation, gate threshold or routing policy may not be promoted from benchmark results alone when production traffic is available.

### 13.5 Rollback

Every deployed policy has:

- immutable version;
- previous-good version;
- activation timestamp;
- model-registry compatibility range;
- rollback trigger thresholds.

Rollback must not require a desktop release.

---

## 14. Primary metrics

### 14.1 North-star metric

```text
total cost + human effort per verified successful task
```

with a required quality floor.

### 14.2 Quality

- verified task success;
- first-pass verified success;
- final verified success;
- user satisfaction;
- keep rate;
- regression escape;
- security regression escape;
- reviewer defect recall;
- gate false-accept rate;
- gate false-reject rate;
- acceptance_false_accept_rate and acceptance_false_reject_rate;
- realized_risk_false_negative_rate and realized_risk_false_positive_rate;
- critical_surface_miss_rate, independently gated from router quality.

### 14.3 Economic

- total inference cost per verified success;
- cost by workflow;
- escalation rate;
- reviewer cost;
- retry/fallback cost;
- cache-read/write cost;
- estimated counterfactual savings;
- observed counterfactual savings where available.

### 14.4 Latency

- routing p50/p95;
- first useful action;
- first patch;
- verification completion;
- final completion;
- cascade penalty on escalated tasks.

### 14.5 Router quality

- `p_floor_success` calibration;
- OOD calibration;
- regret versus best known eligible plan;
- unnecessary-frontier rate;
- harmful-underroute rate;
- harmful-overroute rate;
- model-switch cache penalty.

### 14.6 Safety/reliability

- invalid plan rate;
- budget overrun rate;
- timeout rate;
- cancellation cleanliness;
- partial-apply escape rate;
- unauthorized reviewer effect attempts;
- risk-trigger compliance.

---

## 15. Relational reviewer statistics

Reviewer value depends on solver family and task slice. Store defect/critical/security/API/regression recall, false positives, correlated failure, revision success, sample counts, confidence intervals and measured inference/tool costs and latency in versioned Outcome Statistics. The strongest solver is not automatically the best reviewer. Cross-family selection may be required by policy or supported by measured lower failure correlation. Relational value helps price eligible continuations; it cannot replace confidence-adjusted final-plan quality feasibility or independent acceptance/risk gates.

## 16. Multi-turn behavior

### 16.1 Conversation stickiness

Do not treat every turn as an independent routing problem.

Maintain a session routing state:

```yaml
RoutingSessionState:
  active_model_config: string
  active_plan_id: string
  executed_path_labels: []
  cache_state_ref: optional string
  last_profile_ref: string
  last_route_at: timestamp
  route_epoch: integer
```

### 16.2 Re-route only when justified

Examples:

- new independent task;
- compaction/cache epoch boundary;
- major complexity drift;
- task mode transition;
- tool/vision/context requirement changes;
- provider degradation;
- quality-gate escalation;
- risk transition.

### 16.3 Cache economics

The compiler must account for:

- retained cached prefix;
- cache TTL;
- cache write/read pricing;
- lost prefix reuse when switching;
- required re-prefill tokens;
- context reconstruction latency.

A nominally cheaper model may be more expensive after a cache miss.

---

## 17. Auto-mode UX and enterprise policy

### 17.1 Auto mode

Auto should expose:

- Cost;
- Balance;
- Intelligence;

or organization-defined objective profiles.

The user should not need to select a model to benefit from Auto.

### 17.2 Manual mode

Manual model selection may continue for:

- expert users;
- debugging;
- evaluation;
- reproducibility;
- organization policy.

Manual selection still uses Modbit safety, context, tool, verification and cost boundaries.

### 17.3 Model-label visibility

The effective model/workflow must always be logged internally.

UI visibility is policy-controlled.

During development, canary and enterprise debugging, visibility should be available.

A hidden label must not make routing unauditable.

### 17.4 Organization controls

Organizations may constrain:

- permitted objective modes;
- permitted model/provider families;
- required data residency;
- maximum request budget;
- maximum reasoning effort;
- whether independent review is required for selected repositories/paths;
- whether prevalidated escalation slots are allowed;
- whether manual model selection is allowed;
- routing telemetry retention;
- model-label visibility.

---

## 18. Security and trust rules

1. RequestProfile is advisory metadata, not authorization.
2. ConditionalExecutionPlan cannot grant a capability absent from the canonical policy/capability system.
3. Reviewer findings are untrusted model output until validated.
4. Routing telemetry is not evidence that a security issue is fixed.
5. Model selection never changes secret-access policy.
6. Provider eligibility must satisfy data policy before confidence-feasibility and cost ranking.
7. High-risk path rules may force a minimum workflow/verification profile.
8. A model cannot downgrade its own risk class.
9. A floor-model failure must not cause blind retries on policy-denied or unknown-effect operations.
10. Routing configuration changes are privileged, versioned and auditable.
11. No model/provider credentials are distributed to desktop surfaces, plugins or ordinary execution workers. Gateway-owned inference dispatch is the sole credential boundary.
12. Shadow/counterfactual execution must not duplicate production effects.

---

## 19. Integration with existing Modbit components

### 19.1 Model Gateway

Extend, do not replace.

Add:

- role-aware model registry;
- stable model capability/configuration feed;
- separately versioned Outcome Statistics input from the existing Eval Harness;
- current price feed;
- cache economics;
- health/availability;
- routing-compatible provider metadata;
- per-leg usage/cost attribution.

The Model Gateway remains the only hosted/private provider credential boundary.

### 19.2 Agent Runtime

Extend the canonical Run with:

- RequestProfile reference;
- ConditionalExecutionPlan reference and validated continuation slots;
- active plan leg;
- escalation state;
- reviewer result;
- independent AcceptanceGateResult;
- realized-risk result;
- complete accounting.

The runtime remains the executor. The router does not become a competing agent runtime.

### 19.3 Prompt/Skill Compiler

For each leg compile a role-specific instruction package.

Examples:

- solver instructions;
- reviewer instructions;
- revision instructions.

Reviewer prompt packages must not include capabilities the reviewer cannot exercise.

### 19.4 Context Engine

Provide:

- fast task/repository features for profiling;
- revision-bound solver context;
- bounded reviewer context plus disposable sandbox evidence execution;
- diff/dependency/API impact evidence;
- cache/context-size estimates.

Do not require full repository indexing before routing can begin if bounded fallback features are sufficient.

### 19.5 Execution/worktrees

Autonomous compound workflows use existing isolated worktree/checkpoint semantics.

Do not create a second patch-application system.

### 19.6 Verification

Extend VerificationResult with:

- gate role;
- workflow leg;
- acceptance/rejection reason;
- evidence refs;
- realized-risk refs.

### 19.7 Studio / CLI / Web

Clients display/projection only:

- objective mode;
- workflow state if policy permits;
- current plan status;
- verification/escalation progress;
- effective cost/usage;
- routing diagnostics for developer/admin modes.

The routing classifier/policy weights remain server/control-plane authority.

---


## 24. Failure handling

- Profiler unavailable: deterministic conservative intrinsic profile; compile an initial-leg conditional plan using conservative statistics, hard limits and explicit infeasibility if needed.
- Registry/statistics stale or unavailable: use compatible authenticated snapshots only within freshness/data rules. Low-confidence priors cannot qualify cheap plans. If no quality-feasible plan exists, label QUALITY_FLOOR_INFEASIBLE and choose the best hard-eligible plan; if none is hard-eligible, fail.
- Floor/provider unavailable: at compile time exclude unavailable bindings. During a transaction activate only an already admitted PROVIDER_FAILURE slot after effect/usage reconciliation; otherwise stop safely. Do not compile a fresh branch in the active transaction.
- Reviewer unavailable: required review cannot be silently skipped; activate an already validated substitute or human slot if permitted, otherwise explicit attention/failure. Optional review may be omitted only when the compiled acceptance/slot policy already allows it.
- Acceptance Gate/evidence unavailable: INCONCLUSIVE cannot ACCEPT. Activate only a permitted prevalidated continuation or stop.
- Budget exhausted: never mint new budget. Accept only if current evidence meets all requirements; otherwise exact partial/failed/attention state.
- New risk or policy condition requires an absent branch: REQUIRED_CONTINUATION_UNAVAILABLE; preserve candidate/evidence and reconcile current transaction. A later separately admitted transaction inherits remaining request budget and a new fenced epoch.
- User interruption: cancel active bounded leg/processes, reconcile tools/effects/usage, revoke review resources, preserve canonical Run state and resume only at a valid prevalidated slot boundary.

## 25. Non-goals

This patch does **not** authorize:

- arbitrary workflow generation;
- an LLM that chooses the model on every request;
- mandatory model debate;
- recursive self-critique loops;
- best-of-N by default;
- unlimited retries;
- a second orchestration engine;
- direct provider credentials in clients;
- a mandatory local SLM;
- model names hard-coded into product architecture;
- routing based only on synthetic benchmarks;
- bypassing verification because a stronger model was selected;
- treating model confidence as evidence;
- duplicating production side effects for shadow evaluation;
- giving reviewers canonical or persistent write capability; bounded ephemeral review writes are governed by section 9.5.

---

## 26. Acceptance examples

### Localized edit

A high p_floor_success alone does not select a cheap model. Representative Outcome Statistics must give the whole conditional plan an LCB meeting the mode floor. Compile initial economical solver, targeted static evidence, factual risk, acceptance and any needed human/escalation slots. Initial-to-accept yields a DIRECT label only after verified atomic apply.

### Uncertain medium bug

Compile economical initial solver plus a QUALITY_REJECT stronger-solver slot if the whole plan clears confidence/budget constraints. Failed initial evidence activates that slot, and successful escalation yields a CASCADE path label. Initial-leg failure remains failure in its own outcome record.

### Repository-wide migration

A cheap first attempt with weak confidence or negative complete-task economics is excluded. A frontier initial solver may be cheapest among confidence-feasible plans. If no plan meets the quality floor, choose the best hard-eligible fallback, record QUALITY_FLOOR_INFEASIBLE and never claim the target was met.

### Authentication change

Policy minima and factual auth/authorization changes require assurance, tests and possibly independent review/human approval. The compiled plan includes those slots before execution. Static evidence precedes an Isolated Non-Committing Reviewer with no solver hidden reasoning, a disposable test worktree and no canonical/external effects. Required missing slots prevent acceptance. Review/revision produces a CRITIQUE path label, possibly alongside an escalation label.

### Cached multi-turn conversation

A nominally cheaper alternative must still clear confidence-adjusted quality and remain better after lost cache, refill/cache-write, switching latency and hysteresis. Reevaluate at a permitted boundary; active transactions can use only existing slots. No per-turn unbounded rerouting.

## 27. Current architectural authority

Profile intrinsic task demand, derive canonical legal constraints, compile the cheapest confidence-feasible ConditionalExecutionPlan, execute its initial leg, collect static evidence, derive factual assurance, and activate only prevalidated continuations. Record exact decision inputs and request/leg/gate outcomes; improve future conditional parameters offline with independent gate safety and rollback-ready promotion.

The Model Gateway remains inference/credential boundary; Core owns execution; policy, worktrees/checkpoints, context, skills, verification and recovery remain canonical. Outcome Statistics is derived versioned data under existing owners. DIRECT, CASCADE and CRITIQUE classify executed paths. The Isolated Non-Committing Reviewer can gather evidence in a disposable sandbox but cannot own the canonical workspace or persistent/external effects. No architecture or learned score substitutes for real verification.

## 28. Source section coverage map

Every top-level section of both immutable root patches is carried by the numbered dossier as listed here; `tools/check_dossier.py` D8 fails if a patch section disappears from this map. Section numbers on the left are the patches' own; "27 §n" refers to this document.

| Source | Section | Carried by |
|---|---|---|
| v1.0 §1 | Purpose | 27 §1 |
| v1.0 §2 | Authority amendments | 27 §2; `02_AUTHORITY_AND_DECISIONS.md`; `03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md`; `05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md` |
| v1.0 §3 | Product outcome | 27 §3 |
| v1.0 §4 | Architectural invariants | 27 §4; `81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` |
| v1.0 §5 | Runtime architecture, Policy Envelope | 27 §5; `23_SECURITY_POLICY_EFFECT_LEDGER.md`; `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` |
| v1.0 §6 | Request Profiler | 27 §6; `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md`; doc 38 |
| v1.0 §7 | Execution Plan Compiler (template clauses superseded) | 27 §7; docs 15/38; supersession in `06_EPR_V1_1_SUPERSESSION_DECISION.md` |
| v1.0 §8 | ExecutionPlan contract (now ConditionalExecutionPlan) | 27 §8; `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`; doc 38 |
| v1.0 §9 | Bounded Transaction Executor | 27 §9; `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`; `20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`; docs 23/38 |
| v1.0 §10 | Routing and execution telemetry | 27 §10; docs 30; `34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` |
| v1.0 §11 | Outcome and reward model | 27 §11; doc 34 |
| v1.0 §12 | Counterfactual learning | 27 §12; `53_PERFORMANCE_AND_BENCHMARK_PLAN.md` |
| v1.0 §13 | Offline Policy Lab | 27 §13; `12_REPOSITORY_AND_MODULE_LAYOUT.md`; docs 53/61 |
| v1.0 §14 | Primary metrics | docs 34/53; `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` |
| v1.0 §15 | Critic selection model (now reviewer statistics) | 27 §7.3; docs 06/38 |
| v1.0 §16 | Multi-turn behavior | 27 §7.6; doc 15; `19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` |
| v1.0 §17 | Auto-mode UX and enterprise policy | `10_PRODUCT_PRD_AND_UX.md`; `32_DESKTOP_FRONTEND_IMPLEMENTATION.md`; `24_CLOUD_CONTROL_PLANE_AND_SYNC.md`; doc 30 |
| v1.0 §18 | Security and trust rules | doc 23; `52_SECURITY_THREAT_MODEL_AND_TESTS.md` |
| v1.0 §19 | Integration with existing components | `11_SYSTEM_ARCHITECTURE.md`; docs 12/14/15/18/20/26/33; §19.7 in docs 10/32 |
| v1.0 §20 | Required dossier integration (old file names) | historical: doc 05; `94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md` |
| v1.0 §21 | Implementation work packages | `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` |
| v1.0 §22 | Delivery sequence | `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md`; doc 49 |
| v1.0 §23 | Release gates | doc 61 |
| v1.0 §24 | Failure handling | doc 38 (typed failures); `71_OPERATIONS_RUNBOOK.md`; EPR-FI scenarios in doc 61 |
| v1.0 §25 | Non-goals | 27 (non-goals); doc 81 |
| v1.0 §26 | Acceptance examples | 27 (examples); doc 61 source example fixtures |
| v1.0 §27 | Final architectural decision (superseded by v1.1 §22) | 27 §27; doc 06 |
| v1.0 §28 | Adoption checklist | historical: docs 05/94 |
| v1.1 §1 | Purpose | doc 06; `00_MASTER_INDEX.md` |
| v1.1 §2 | Normative architecture after v1.1 | doc 11 conditional execution flow; 27 §5–9 |
| v1.1 §3 | Supersession: workflow templates | 27 §2.3, §8; doc 06; ADR-R-049 in doc 02 |
| v1.1 §4 | Supersession: quality objective | 27 §7.5; doc 38 CompileConfidenceFeasiblePlan; ADR-R-050 |
| v1.1 §5 | Cold-start rule | 27 §7.3; doc 61 thresholds and priors |
| v1.1 §6 | Model Registry vs Outcome Statistics | 27 §7.2–7.3; docs 31/38; ADR-R-051 |
| v1.1 §7 | Supersession: escalation probability | 27 §7.3; doc 38 |
| v1.1 §8 | Supersession: risk handling | 27 §5.1, §9.3; docs 23/38; ADR-R-052 |
| v1.1 §9 | Realized Risk vs Acceptance Gate | 27 §9.3–9.4; docs 33/38; ADR-R-052 |
| v1.1 §10 | Reviewer terminology and capability model | 27 §9.5; docs 16/21/23; ADR-R-053 |
| v1.1 §11 | Evidence-first review ordering | 27 §9.2; doc 38 |
| v1.1 §12 | Prevalidated continuation slots | 27 §7.7, §8; doc 38 ValidateConditionalExecutionPlan; ADR-R-054 |
| v1.1 §13 | Multi-turn routing refinement | 27 §7.6; docs 15/19 |
| v1.1 §14 | Logging changes | 27 §10; docs 30/34/38 |
| v1.1 §15 | Reward attribution | 27 §11.3; doc 34; ADR-R-055 |
| v1.1 §16 | Gate evaluation becomes release-critical | 27 §13.4; doc 61 gate G and EPR-019; `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md`; ADR-R-056 |
| v1.1 §17 | Updated policy-search space | 27 §13.2; doc 61 |
| v1.1 §18 | Required amendments to v1.0 | doc 06; `v1.1:18` references in doc 49 |
| v1.1 §19 | ADR amendments | doc 02 |
| v1.1 §20 | Task delta | EPR-014..019 in doc 49 |
| v1.1 §21 | Integration prompt | `95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md` |
| v1.1 §22 | Final authority statement | 27 §27 |
