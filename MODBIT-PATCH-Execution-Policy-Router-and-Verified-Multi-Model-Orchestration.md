# Modbit Dossier Patch - Execution Policy Router and Verified Multi-Model Orchestration

**Patch ID:** MODBIT-PATCH-EPR-2026-09-05  
**Version:** 1.0  
**Status:** READY FOR ADOPTION  
**Product:** Modbit  
**Scope:** Agent Runtime, Model Gateway, execution policy, verification, evaluation, economic optimization  
**Authority after adoption:** This patch supersedes only the conflicting routing/reasoning/economic rules identified in Section 2. All other current dossier invariants remain in force.  
**Implementation posture:** Clean-room, provider-neutral, model-name-neutral, evaluation-gated.

---

## 1. Purpose

This patch upgrades Modbit from **single-model adaptive routing** to a **verified execution-policy architecture**.

The target is not merely to select the best model for a request. The target is to select the **least expensive, sufficiently fast, policy-compliant execution plan that is expected to produce a verified successful outcome**.

The core runtime becomes:

```text
                    POLICY ENVELOPE
          identity / org rules / permissions /
         hard risk rules / budgets / data policy
                           |
                           v
Request -> [1] Request Profiler -> [2] Execution Plan Compiler
                                            |
                                            v
                                 [3] Bounded Transaction Executor
                                            |
                                            v
                                  Verified Result + Evidence
                                            |
                                            v
                                  Outcome / Reward Telemetry
                                            |
                     +----------------------+
                     |
                     v
              OFFLINE POLICY LAB
     calibration / counterfactual replay /
     workflow search / holdout / shadow /
          A/B / canary / rollback
```

The runtime has **one orchestration core**. This patch does not introduce a second harness, a second agent graph, or a meta-agent that plans the harness.

The new routing system compiles a bounded execution policy into the existing canonical Run/Session, Model Gateway, worktree, checkpoint, capability, tool, verification and recovery machinery.

---

## 2. Authority amendments

### 2.1 Superseded rule: frontier model as sole production reasoning intelligence

The previous rule:

> The frontier LLM is the sole production reasoning intelligence.

is superseded.

Replace it with:

> **Modbit may use multiple eligible reasoning models through a governed Execution Policy Router. A frontier model remains the quality backstop, not a mandatory first-hop for every request. No model may bypass Modbit policy, capability, context, write-safety, verification, credential, or effect boundaries.**

This amendment does **not** require a local small model. The economical floor, frontier solver, critic or specialist may be hosted, private, custom-compatible or otherwise available through the Model Gateway.

The existing rule that a mandatory local SLM is not a production dependency remains unchanged.

### 2.2 Superseded rule: frontier tokens as the primary efficiency metric

Replace:

> frontier-model tokens per verified successful outcome

with:

> **total inference cost + human intervention + wall-clock per verified successful outcome, subject to a required quality floor.**

Also report model-token detail, cache cost, retries, escalation, critic cost, deterministic verification cost, and the estimated or observed counterfactual cost of a direct frontier execution.

### 2.3 Extended rule: adaptive routing

The existing evaluation-gated adaptive-routing rule is retained and expanded:

> **Adaptive routing includes model selection and bounded workflow selection. DIRECT, CASCADE and CRITIQUE are the initial approved workflow families. New workflow families require benchmark evidence, holdout validation, production shadowing or equivalent counterfactual evidence, and explicit promotion.**

---

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
solver A -> independent critic B -> solver A revision
```

The critic is selected for defect-detection value and diversity of failure mode, not merely raw coding benchmark strength.

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
4. **One execution core.** DIRECT, CASCADE and CRITIQUE compile into the existing Run/Session and execution runtime.
5. **No arbitrary generated workflow DAGs in the initial architecture.**
6. **No unbounded model debates, committees, recursive critics or best-of-N tournaments by default.**
7. **Every leg is bounded.** Every model leg has a token/output budget, timeout, cancellation signal and retry ceiling.
8. **Budget inheritance.** Escalation receives the remaining request budget, not a fresh unlimited budget.
9. **Critics are effect-less.** A critic cannot mutate files, invoke write-capable tools, use credentials, deploy, commit or cause external side effects.
10. **Verification is evidence-first.** Deterministic checks are preferred to LLM judging whenever a reliable deterministic oracle exists.
11. **Autonomous mutation uses isolated worktree/checkpoint semantics.**
12. **No completion based on model confidence alone.**
13. **Risk is policy-owned.** A learned scalar never grants permission or weakens a required review/verification path.
14. **Routing is cache-aware.**
15. **Routing changes are evaluation-gated and versioned.**
16. **Hosted provider credentials remain behind the Model Gateway boundary.**
17. **Manual model selection may remain available outside Auto mode when product/org policy permits it. Auto mode exposes objectives, not a requirement to pick model names.**
18. **Routing telemetry never becomes authorization authority.**
19. **Routing failure fails safe to a validated fallback plan or explicit error; it never silently executes an invalid plan.**
20. **No fixed model name is normative architecture. Models occupy roles.**

---

## 5. Runtime architecture

### 5.1 Layer 0 - Policy Envelope

Before routing, derive a policy envelope from canonical identity, organization, repository, mode and task state.

```yaml
PolicyEnvelope:
  tenant_id: string
  user_id: string
  repository_id: string
  run_id: string

  allowed_providers: []
  allowed_models: []
  blocked_models: []

  data_residency: optional
  retention_class: optional

  cost_cap:
    request_max: money
    daily_remaining: optional money

  latency_policy:
    mode: cost | balance | intelligence | custom
    deadline_ms: optional integer

  permission_mode: supervised | autonomous | governed
  verification_profile: FAST | STANDARD | GOVERNED | HIGH_ASSURANCE

  risk_prior:
    class: low | medium | high | critical
    reasons: []

  required_checks: []
  forbidden_effects: []
```

The Policy Envelope is deterministic/policy-owned. It cannot be weakened by the Request Profiler or any model.

---

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

## 7. Layer 2 - Execution Plan Compiler

### 7.1 Mission

Combine:

- RequestProfile;
- PolicyEnvelope;
- Model Registry;
- Skill Registry;
- current cache/session state;
- provider health;
- current prices;
- latency observations;
- budgets;
- verification capabilities;

and emit a validated `ExecutionPlan`.

This layer is deterministic given its versioned inputs.

### 7.2 Model Registry

The registry is configuration/data, not model-specific routing code.

```yaml
ModelProfile:
  model_id: string
  revision: string
  provider_id: string

  roles:
    - ECONOMICAL_FLOOR
    - FRONTIER_SOLVER
    - CRITIC
    - SPECIALIST

  capabilities:
    reasoning: float
    code_generation: float
    debugging: float
    tool_use: float
    vision: float
    long_context: float

  empirical:
    task_success_by_slice: {}
    first_pass_success_by_slice: {}
    verified_success_by_slice: {}
    critic_defect_recall_by_solver_family: {}
    critic_false_positive_rate: optional float
    cascade_recovery_rate: optional float
    tool_reliability: optional float

  context:
    normal_limit: integer
    extended_limit: optional integer

  cost:
    input_per_token: money
    output_per_token: money
    cache_read_per_token: optional money
    cache_write_per_token: optional money

  latency:
    p50_ms: integer
    p95_ms: integer

  availability:
    status: healthy | degraded | unavailable
    recent_error_rate: float

  governance:
    allowed_tenants: optional
    blocked_tenants: optional
    data_policy_tags: []

  family: string
```

Capability values may start from benchmark priors but must increasingly become empirical and versioned.

### 7.3 Skill-aware configuration

The compiler may evaluate an eligible combination as:

```text
model x skill-pack x reasoning-effort x context-tier
```

without changing the RequestProfile.

A Skill may improve expected task success, but it does not change capability permissions.

### 7.4 Model roles

#### ECONOMICAL_FLOOR

A model/configuration eligible for inexpensive first-pass work.

It must satisfy minimum thresholds for:

- tool reliability;
- context requirement;
- output format reliability;
- first-pass verified success on supported slices;
- provider health.

If no valid floor exists, CASCADE is not eligible.

#### FRONTIER_SOLVER

A high-quality solver configuration used for:

- difficult direct execution;
- escalation;
- high-assurance work;
- OOD or low-confidence tasks where a floor attempt has negative expected value.

#### CRITIC

A review configuration optimized for:

- defect recall;
- low false-positive rate;
- complementary failure modes relative to the solver;
- security/API/regression detection where applicable;
- low enough cost/latency to justify review.

The strongest solver is not automatically the best critic.

### 7.5 Approved initial workflow catalog

Only these are P0/P1 workflow families:

```text
DIRECT(model_config)

CASCADE(
  floor_config,
  quality_gate,
  escalation_config
)

CRITIQUE(
  solver_config,
  critic_config,
  revision_config
)
```

A plan may also include deterministic verification before final completion.

No arbitrary recursive orchestration is required.

### 7.6 Candidate plan generation

The compiler enumerates only eligible plans.

Example:

```text
DIRECT(floor)
DIRECT(frontier_A)
DIRECT(frontier_B)
CASCADE(floor -> frontier_A)
CASCADE(floor -> frontier_B)
CRITIQUE(frontier_A, critic_B, frontier_A)
CRITIQUE(frontier_B, critic_A, frontier_B)
```

Plans that violate policy, capability, budget, context, availability or workflow constraints are removed before scoring.

### 7.7 Objective

Prefer constrained optimization over hard-coded routing trees.

A practical baseline:

```text
maximize:
  E[verified_quality(plan | request)]
  - lambda_cost * E[cost(plan)]
  - lambda_latency * E[latency(plan)]

subject to:
  worst_case_cost(plan) <= request_cost_cap
  expected_failure(plan) <= mode_failure_budget
  plan.models subset_of allowed_models
  plan.effects subset_of permitted_effects
  required_verification(plan) is satisfiable
  context(plan) is compatible
  provider_health(plan) is acceptable
  risk_rules(plan) are satisfied
```

Risk is primarily enforced through constraints and required checks, not a learned scalar penalty.

### 7.8 Optimization modes

Auto mode exposes objective modes.

#### COST

- highest cost pressure;
- still subject to minimum verified quality;
- favors economical floor and cascade when expected value is positive.

#### BALANCE

- default;
- balanced quality/cost/latency;
- higher escalation willingness than COST.

#### INTELLIGENCE

- high quality floor;
- lower cost pressure;
- more direct frontier and high-value critique;
- still bounded and budgeted.

Organizations may define custom weights and disable modes.

### 7.9 Cache-aware effective cost

When comparing staying on an active model versus switching, compute effective cost rather than nominal per-token price.

```text
effective_switch_cost =
    new_uncached_prefill_cost
  + cache_write_cost_if_any
  + expected_output_cost
  + expected_latency_penalty
  - avoided_old_model_cost
```

The compiler should prefer conversation-level stickiness when the expected quality gain from switching does not exceed cache/refill/latency cost.

### 7.10 Re-routing boundaries

Do not independently re-route every turn.

Preferred re-routing boundaries:

- start of a new request/Run;
- after context compaction/epoch transition;
- explicit user mode change;
- active model unavailable/degraded;
- complexity drift above threshold;
- escalation triggered by quality gate;
- policy/risk transition;
- sufficiently large task decomposition boundary.

The plan records why re-routing occurred.

### 7.11 Validated routing

Before execution, validate:

- every model binding exists;
- model revision is eligible;
- required capabilities are present;
- floor role is valid when CASCADE is used;
- critic and solver isolation rules are valid;
- cross-family requirement if configured;
- fallback chain has no cycle;
- context window is sufficient;
- data policy is satisfied;
- worst-case cost fits remaining request budget;
- retry ceilings are finite;
- timeouts are finite;
- deterministic verification dependencies are available;
- required worktree/checkpoint mode is available.

Invalid plan -> compile next eligible plan or fail explicitly.

---

## 8. ExecutionPlan contract

```yaml
ExecutionPlan:
  plan_id: uuid
  plan_version: string
  policy_version: string
  profiler_version: string
  model_registry_version: string
  skill_registry_version: string

  run_id: uuid
  workflow: DIRECT | CASCADE | CRITIQUE
  objective_mode: COST | BALANCE | INTELLIGENCE | CUSTOM

  request_profile_ref: string
  policy_envelope_ref: string

  legs:
    - leg_id: string
      role: solver | critic | reviser | escalation
      model_config_ref: string
      skill_refs: []
      context_policy_ref: string
      tool_capability_ref: string
      max_input_tokens: integer
      max_output_tokens: integer
      timeout_ms: integer
      max_retries: integer
      can_mutate_workspace: boolean
      can_use_external_effects: boolean

  quality_gate_ref: optional string
  verification_profile: FAST | STANDARD | GOVERNED | HIGH_ASSURANCE

  workspace_policy:
    isolated_worktree: boolean
    checkpoint_before: boolean
    atomic_apply: boolean

  budget:
    request_cost_cap: money
    reserved_worst_case: money
    remaining_at_compile: money

  fallback_plan_refs: []

  compiler_reason:
    primary_factors: []
    excluded_plans: []
```

The plan is durable provenance, but it does not replace the original user request.

---

## 9. Layer 3 - Bounded Transaction Executor

### 9.1 Mission

Execute the compiled plan through the existing Agent Runtime and Model Gateway while preserving:

- canonical Run/Session state;
- worktree/revision identity;
- capability enforcement;
- checkpoints;
- typed tool execution;
- approvals;
- verification;
- failure classification;
- cost accounting;
- recovery.

### 9.2 Per-leg bounds

Every leg receives:

- input/context ceiling;
- output ceiling;
- wall-clock timeout;
- cancellation token;
- retry ceiling;
- tool/capability manifest;
- remaining budget;
- permitted effect class.

No leg may silently obtain a new full request budget.

### 9.3 Budget inheritance

For CASCADE:

```text
request budget = B

floor leg spends F
quality gate spends G

escalation maximum <= B - F - G - reserved mandatory verification
```

The same principle applies to CRITIQUE.

### 9.4 Solver capabilities

A solver may receive the normal task-scoped capability surface permitted by policy:

- read;
- search;
- edit;
- terminal;
- build/test;
- browser;
- other registered engineering tools;
- external effects only when explicitly allowed and governed.

### 9.5 Critic isolation

Critics are **effect-less**.

A critic may receive:

- original task;
- acceptance criteria;
- candidate diff;
- diagnostics;
- build/test results;
- selected source excerpts;
- bounded read-only Context Engine retrieval;
- public API/dependency impact;
- security signals;
- relevant prior decisions.

A critic may not:

- mutate repository files;
- write through tools;
- commit/push;
- deploy;
- send external messages;
- access secrets unless a separate explicit policy permits a non-effectful secret-safe review representation;
- change approvals;
- expand its own capabilities.

Critic output is a structured finding set, not direct mutation.

### 9.6 Critic output contract

```yaml
CriticResult:
  verdict: pass | revise | block | uncertain
  confidence: float

  findings:
    - finding_id: string
      severity: info | low | medium | high | critical
      category: correctness | regression | security | api | scope | test_gap | other
      claim: string
      evidence_refs: []
      suggested_action: optional string

  unresolved_questions: []
```

The runtime validates critic findings before using them as completion evidence.

### 9.7 CASCADE quality gate

The gate should be cheap and deterministic-first.

Preferred inputs:

- compile/build;
- typecheck;
- lint;
- targeted tests;
- existing regression tests;
- explicit task acceptance checks;
- scope-change check;
- unexpected file-touch check;
- API signature delta;
- dependency/lockfile delta;
- protected-path touch;
- diagnostics delta;
- security signal delta;
- explicit model uncertainty marker;
- tool errors;
- incomplete TODO markers;
- unhandled failures.

A small model judge may be used only when deterministic evidence is insufficient and evaluation proves net benefit.

### 9.8 Gate output

```yaml
QualityGateResult:
  verdict: accept | reject | uncertain
  reasons: []
  deterministic_evidence_refs: []
  judge_ref: optional
  false_accept_risk_estimate: optional
```

`uncertain` may trigger escalation when budget/policy permits.

### 9.9 Realized risk

The pre-execution policy contains `risk_prior`.

After a candidate diff exists, derive `risk_actual` from facts such as:

- protected files touched;
- authentication/authorization code touched;
- secrets/credential code touched;
- CI/CD or infrastructure changed;
- dependency/lockfile changed;
- schema/migration changed;
- public API changed;
- file/line blast radius;
- test coverage gap;
- security findings;
- unexpected scope expansion;
- external effects requested.

`risk_actual` may strengthen required verification or force critique/escalation. It may not weaken the original policy minimum.

### 9.10 Atomic application

For autonomous mutation:

```text
starting revision
      |
      v
isolated worktree / checkpoint
      |
      v
solve -> verify -> critique/revise if required
      |
      v
acceptance gate
   /       \
PASS       FAIL/CANCEL
 |             |
apply          discard/rollback
```

No failed compound workflow should leave a partially accepted repository patch.

External irreversible side effects remain governed by the existing policy/effect/recovery mechanisms and cannot be treated as ordinary worktree rollback.

### 9.11 Completion

A model may propose completion.

The runtime determines completion only after:

- required workflow legs complete;
- required verification completes;
- quality gate accepts where configured;
- critical critic findings are resolved;
- write-safety conditions hold;
- acceptance criteria are satisfied or unresolved exceptions are explicitly surfaced.

---

## 10. Routing and execution telemetry

Every request records a single top-level routing outcome plus leg-level detail.

```yaml
RoutingTrace:
  request_id: uuid
  run_id: uuid

  policy_version: string
  profiler_version: string
  model_registry_version: string

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

#### Explicit
- thumbs/ratings;
- user-reported failure;
- explicit accept/reject.

#### Implicit
- edit kept;
- edit reverted within a configured window;
- immediate corrective follow-up;
- user manually switches model after poor result;
- repeated repair turns;
- intervention during autonomous run.

#### Hard outcome
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

### 13.2 Workflow search space

Initially search only:

- workflow family: DIRECT / CASCADE / CRITIQUE;
- floor model/config;
- frontier model/config;
- critic model/config;
- reasoning effort;
- context tier;
- skill pack;
- cascade threshold;
- gate threshold;
- critic threshold;
- output budget;
- timeout;
- retry ceiling.

Do not search arbitrary generated agent graphs.

### 13.3 Search objective

Two-stage optimization is preferred.

#### Stage A - reach quality feasibility

Find configurations meeting the required verified-quality floor and safety constraints.

#### Stage B - optimize on the feasible boundary

Among feasible configurations, reduce:

- cost;
- latency;
- intervention;
- retry burden;

without dropping below the quality floor.

Beam search, constrained hill-climbing, Bayesian optimization or another bounded configuration-search method may be used. The algorithm is replaceable; the evaluation contract is not.

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
- critic defect recall;
- gate false-accept rate;
- gate false-reject rate.

### 14.3 Economic

- total inference cost per verified success;
- cost by workflow;
- escalation rate;
- critic cost;
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
- unauthorized critic effect attempts;
- risk-trigger compliance.

---

## 15. Critic selection model

Critic quality is relational.

The important question is not:

> What is the strongest critic model?

It is:

> Which eligible critic detects failures made by this solver at acceptable cost and latency?

Track:

```yaml
CriticProfile:
  critic_model_config: string
  solver_family: string

  defect_recall: float
  false_positive_rate: float
  critical_defect_recall: float

  security_recall: optional float
  api_break_recall: optional float
  regression_recall: optional float

  correlation_with_solver_failures: float

  median_cost: money
  p95_latency_ms: integer
```

A useful conceptual score is:

```text
critic_value(solver, critic)
  = expected_detected_solver_failures
  - cost_weight * critic_cost
  - latency_weight * critic_latency
  - false_positive_weight * false_positive_rate
```

Cross-family review may be preferred when empirical data shows lower correlated failure.

---

## 16. Multi-turn behavior

### 16.1 Conversation stickiness

Do not treat every turn as an independent routing problem.

Maintain a session routing state:

```yaml
RoutingSessionState:
  active_model_config: string
  active_workflow: string
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
- whether CRITIQUE is required for selected repositories/paths;
- whether CASCADE is allowed;
- whether manual model selection is allowed;
- routing telemetry retention;
- model-label visibility.

---

## 18. Security and trust rules

1. RequestProfile is advisory metadata, not authorization.
2. ExecutionPlan cannot grant a capability absent from the canonical policy/capability system.
3. Critic findings are untrusted model output until validated.
4. Routing telemetry is not evidence that a security issue is fixed.
5. Model selection never changes secret-access policy.
6. Provider eligibility must satisfy data policy before utility scoring.
7. High-risk path rules may force a minimum workflow/verification profile.
8. A model cannot downgrade its own risk class.
9. A floor-model failure must not cause blind retries on policy-denied or unknown-effect operations.
10. Routing configuration changes are privileged, versioned and auditable.
11. No model/provider credentials are distributed to Studio, plugins or ordinary workers.
12. Shadow/counterfactual execution must not duplicate production effects.

---

## 19. Integration with existing Modbit components

### 19.1 Model Gateway

Extend, do not replace.

Add:

- role-aware model registry;
- model capability/empirical profile feed;
- current price feed;
- cache economics;
- health/availability;
- routing-compatible provider metadata;
- per-leg usage/cost attribution.

The Model Gateway remains the only hosted/private provider credential boundary.

### 19.2 Agent Runtime

Extend the canonical Run with:

- RequestProfile reference;
- ExecutionPlan reference;
- active plan leg;
- escalation state;
- critic result;
- quality-gate result;
- realized-risk result;
- complete accounting.

The runtime remains the executor. The router does not become a competing agent runtime.

### 19.3 Prompt/Skill Compiler

For each leg compile a role-specific instruction package.

Examples:

- solver instructions;
- critic instructions;
- revision instructions.

Critic prompt packages must not include capabilities the critic cannot exercise.

### 19.4 Context Engine

Provide:

- fast task/repository features for profiling;
- revision-bound solver context;
- bounded critic read-only retrieval;
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

## 20. Required dossier integration

On adoption, reconcile the current dossier as follows.

### 20.1 `06-Decision-Register.md`

Add the following decisions.

```text
ADR-R-039 LOCKED
Modbit uses a provider-neutral Execution Policy Router. The router may select
DIRECT, CASCADE or CRITIQUE plans and bind eligible model/skill configurations.
No fixed model name is normative architecture.

ADR-R-040 LOCKED
The Request Profiler predicts task/capability requirements and floor-success
probability independent of model price, availability, cache residency and
organization policy.

ADR-R-041 LOCKED
The Execution Plan Compiler is deterministic over versioned profiler output,
policy, model/skill registries, cache state, price, latency, health and budgets.

ADR-R-042 LOCKED
CASCADE and CRITIQUE execute through the canonical Agent Runtime. They do not
create a second orchestration core.

ADR-R-043 LOCKED
Critics are effect-less. They may use bounded read-only repository/context
retrieval but cannot mutate the workspace or cause external side effects.

ADR-R-044 LOCKED
Routing and workflow changes are evaluation-gated. Production promotion requires
holdout evidence and, when production traffic exists, shadow/canary/A-B evidence
or an explicitly approved equivalent.

ADR-R-045 LOCKED
The primary economic metric is total inference cost plus human intervention and
wall-clock per verified successful task, subject to a quality floor.

ADR-R-046 LOCKED
Risk remains policy-owned. Pre-execution risk prior and post-draft realized risk
may force stronger verification, critique or escalation; learned routing cannot
weaken policy.

ADR-R-047 LOCKED
Auto-mode routing is cache-aware and sticky across multi-turn epochs; model
switching occurs only when expected utility exceeds cache/refill and latency cost.

ADR-R-048 LOCKED
Every compound request is completely accounted: all solver, critic, revision,
escalation, retry and fallback legs are included in cost and telemetry.
```

Mark the old frontier-only reasoning decision as **SUPERSEDED BY ADR-R-039..048**.

Mark the old frontier-token-only efficiency decision as **SUPERSEDED BY ADR-R-045**.

### 20.2 `09-Agent-Runtime-and-Model-Gateway.md`

Replace the frontier-only rule with Sections 2-9 of this patch.

Replace "Provider routing" with "Execution Policy Routing".

Update the core loop to:

```text
observe task/state
 -> derive PolicyEnvelope
 -> build RequestProfile
 -> compile/validate ExecutionPlan
 -> compile context/prompt/skills/tools for active leg
 -> invoke active model through Model Gateway
 -> validate requested actions
 -> execute within leg capabilities/budget
 -> observe exact result
 -> persist state/events/accounting
 -> run gate/verification/risk checks
 -> accept | revise | escalate | continue | ask | fail
```

Replace "frontier model may request additional context/Skills/tools" with:

> the active solver/reviser model may request governed progressive disclosure;
> critics receive only the read-only context surface defined by their plan.

Replace "frontier model proposes completion" with:

> an active solver/reviser may propose completion; the runtime decides whether
> the compiled workflow and verification obligations permit completion.

### 20.3 `12-Evidence-Evaluation-and-Robustness.md`

Add:

- router calibration metrics;
- workflow regret;
- gate false-accept/false-reject;
- complete compound cost;
- counterfactual replay;
- propensity logging;
- critic evaluation;
- escalation recovery rate;
- cache-aware economic evaluation;
- policy version rollback.

Replace the primary economic metric with Section 14 of this patch.

### 20.4 `14-Algorithms.md`

Add canonical algorithms for:

1. RequestProfiler;
2. EnumerateEligiblePlans;
3. ScoreExecutionPlan;
4. ValidateExecutionPlan;
5. ExecuteDirect;
6. ExecuteCascade;
7. ExecuteCritique;
8. EvaluateQualityGate;
9. DeriveRealizedRisk;
10. CompleteAccounting;
11. CounterfactualReplay;
12. PromoteRoutingPolicy.

### 20.5 `04-Task-Breakdown.md`

Add the implementation work packages in Section 21.

### 20.6 `13-Capability-Matrix-and-Migration.md`

Change "Agent Runtime -> real frontier-model/tool executor" to:

> real policy-routed model/tool executor with frontier quality backstop.

Change "SLM/local intelligence" only if necessary to clarify:

> no mandatory local model; economical routing roles may use any eligible
> provider-neutral model configuration that passes evaluation.

---

## 21. Implementation work packages

### EPR-001 - Routing contracts

**Scope**

Implement/version:

- PolicyEnvelope;
- RequestProfile;
- ModelProfile;
- ExecutionPlan;
- RoutingTrace;
- QualityGateResult;
- CriticResult;
- OutcomeRecord.

**Acceptance**

- schema validation;
- backwards-compatible Run persistence;
- no model/provider credential leakage;
- version fields mandatory.

### EPR-002 - Model Registry

**Scope**

Add provider-neutral roles, capability/empirical profile, economics, latency, health and governance metadata.

**Acceptance**

- registry updates require no desktop release;
- invalid/blocked models cannot compile into plans;
- stale registry generation is detectable.

### EPR-003 - Bootstrap Request Profiler

**Scope**

Implement initial task/domain/modifier features, `p_floor_success`, uncertainty and OOD.

**Acceptance**

- no general LLM call on request hot path;
- latency target met;
- calibration benchmark published;
- deterministic fallback profile exists.

### EPR-004 - Execution Plan Compiler

**Scope**

Implement candidate enumeration, constraints, utility scoring, cache economics and validation.

**Acceptance**

- deterministic output for identical versioned inputs;
- no cyclic fallback;
- worst-case budget bounded;
- invalid plans rejected before execution.

### EPR-005 - DIRECT integration

**Scope**

Route current single-model path through ExecutionPlan.

**Acceptance**

- no material regression versus current direct baseline;
- full accounting;
- manual model pin still works when allowed;
- existing Run/Session and verification semantics preserved.

### EPR-006 - CASCADE

**Scope**

Implement economical first leg, deterministic-first quality gate and bounded escalation.

**Acceptance**

- failed floor attempt cannot exceed request budget;
- escalation consumes remaining budget;
- gate false-accept/false-reject benchmark exists;
- CASCADE must beat direct baseline on at least one approved quality/cost slice before promotion.

### EPR-007 - CRITIQUE

**Scope**

Implement solver -> effect-less critic -> bounded revision.

**Acceptance**

- critic cannot mutate;
- critic read-only context is revision-bound;
- critic finding contract validated;
- measured defect-recall uplift on approved slices;
- workflow cost completely accounted.

### EPR-008 - Risk integration

**Scope**

Implement policy-owned `risk_prior` and factual post-draft `risk_actual`.

**Acceptance**

- critical path types force configured minimum verification;
- learned profiler cannot downgrade risk;
- protected-path tests pass.

### EPR-009 - Cache-aware multi-turn routing

**Scope**

Track routing epochs, cache state and switch economics.

**Acceptance**

- no per-turn independent rerouting by default;
- switching reason logged;
- cache-miss cost included in evaluation;
- compaction epoch can trigger re-route.

### EPR-010 - Outcome telemetry

**Scope**

Capture verified outcome, corrections, reverts, tests, intervention, cost and plan provenance.

**Acceptance**

- plan/policy/model-registry versions reconstructable;
- raw reward signals retained;
- privacy/retention policy applied.

### EPR-011 - Counterfactual replay

**Scope**

Implement snapshot-safe alternative-plan replay.

**Acceptance**

- no production side effects;
- no credentials;
- exact revision identity;
- observed counterfactual distinguished from estimated counterfactual.

### EPR-012 - Policy Lab and rollout gates

**Scope**

Implement offline plan search, holdout, shadow/canary/A-B promotion and rollback.

**Acceptance**

- production policy version can roll back without desktop release;
- promotion requires named evidence set;
- quality-floor regression blocks rollout.

### EPR-013 - Skill-aware routing

**Scope**

Allow evaluated model x Skill combinations to have distinct empirical success profiles.

**Acceptance**

- Skills never grant capabilities;
- skill versions included in plan provenance;
- only evaluation-qualified combinations influence routing.

---

## 22. Delivery sequence

Do not implement all compound workflows at once.

### Phase 0 - Preserve baseline

- keep current direct execution working;
- instrument current model calls;
- establish complete cost and verified-outcome baseline.

### Phase 1 - Unify routing path

- implement contracts;
- Model Registry;
- Request Profiler in shadow;
- Execution Plan Compiler;
- route existing direct path through `DIRECT`.

At this phase, product behavior may remain almost unchanged.

### Phase 2 - Floor routing

- qualify an ECONOMICAL_FLOOR role;
- enable `DIRECT(floor)` only on high-confidence slices;
- retain frontier direct fallback.

### Phase 3 - CASCADE

- deterministic quality gate;
- bounded escalation;
- evaluate uncertain/medium requests.

### Phase 4 - CRITIQUE

- effect-less critic;
- target high-risk/high-value slices where measured defect recall justifies cost.

### Phase 5 - Learning loop

- production reward;
- propensity;
- counterfactual replay;
- policy search;
- controlled exploration.

### Phase 6 - Skill-aware optimization

- model x Skill empirical profiles;
- route qualified skill packs without changing permission authority.

---

## 23. Release gates

The feature is not "done" when the router compiles.

### Gate A - Correctness

- schemas stable;
- deterministic plan compilation;
- invalid plans fail closed;
- cancellation/timeout/retry bounded;
- no partial workspace apply.

### Gate B - Direct-path non-regression

`DIRECT` through the new compiler must match or exceed the current baseline for:

- verified success;
- latency within approved tolerance;
- cost within approved tolerance;
- recovery;
- tool reliability.

### Gate C - Profiler calibration

Required calibration thresholds are set and met on untouched holdout data.

### Gate D - CASCADE benefit

CASCADE must demonstrate:

- required verified-quality floor;
- lower total expected cost on target slice;
- acceptable latency;
- bounded gate false-accept rate.

### Gate E - CRITIQUE benefit

CRITIQUE must demonstrate:

- statistically meaningful defect-recall or verified-success gain on target slice;
- acceptable false-positive/revision churn;
- acceptable incremental cost/latency.

### Gate F - Multi-turn economics

Switching policy must show:

- cache/refill accounting;
- no material cost regression from unnecessary switches;
- acceptable quality-drift detection.

### Gate G - Production rollout

- shadow/canary/A-B evidence;
- rollback tested;
- monitoring alerts active;
- policy version pinned and reconstructable.

---

## 24. Failure handling

### Profiler unavailable

Use deterministic fallback profile and compile a conservative DIRECT plan.

### Model Registry stale/unavailable

Use last-known-good signed/versioned registry if within freshness policy; otherwise fail routing and use configured safe fallback.

### Floor unavailable

Remove CASCADE/floor candidates and recompile.

### Critic unavailable

If critique is optional, recompile an eligible plan. If critique is policy-required, fail or use an approved critic fallback.

### Quality gate unavailable

If the gate is mandatory for CASCADE, do not auto-accept the floor result. Escalate or fail according to policy/budget.

### Budget exhausted

Do not mint new budget. Complete only if current evidence satisfies completion requirements; otherwise fail/return partial with exact state.

### Provider degradation mid-leg

Use existing typed provider failover only if the replacement satisfies the current plan/policy. Otherwise recompile from a valid boundary.

### User interruption

Cancel active leg, preserve canonical Run state, resolve pending tools/effects through existing recovery semantics, and resume only from a valid plan boundary.

---

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
- giving critics write capability.

---

## 26. Acceptance examples

### Example A - simple localized edit

```text
profile:
  p_floor_success = 0.97
  confidence = high
  risk_prior = low

plan:
  DIRECT(economical_floor)

verification:
  targeted typecheck/test

result:
  accepted if verification passes
```

### Example B - uncertain medium bug

```text
profile:
  p_floor_success = 0.72
  confidence = medium
  ood = low

plan:
  CASCADE(
    economical_floor,
    deterministic_gate,
    frontier_solver
  )

outcome:
  floor passes -> finish cheaply
  floor fails/uncertain -> frontier escalation
```

### Example C - clearly difficult repository-wide migration

```text
profile:
  p_floor_success = 0.18
  context_pressure = high
  long_horizon = true

plan:
  DIRECT(frontier_solver)

reason:
  cheap first attempt has negative expected value
```

### Example D - authentication change

```text
risk_prior:
  high

plan:
  CRITIQUE(
    frontier_solver_A,
    diverse_effectless_critic_B,
    frontier_solver_A
  )

required:
  auth tests
  regression checks
  security signals
  realized-risk evaluation
```

### Example E - multi-turn cached conversation

```text
current:
  model_A has useful prompt/cache state

new turn:
  nominally cheaper model_B exists

compiler:
  predicts only small quality/cost benefit from switch
  includes re-prefill/cache miss cost

plan:
  remain on model_A
```

---

## 27. Final architectural decision

After adoption, the normative Modbit inference strategy is:

> **Profile the request cheaply, compile the best eligible bounded execution plan deterministically, execute it through the canonical Modbit runtime, verify before completion, and learn from verified production outcomes offline.**

The initial workflow library is deliberately small:

```text
DIRECT
CASCADE
CRITIQUE
```

The system optimizes **verified task outcomes**, not model prestige, single-call token counts or router cleverness.

The strongest frontier model is the quality backstop. It is not automatically the first or only reasoning model.

The economical floor is a role, not a hard-coded model.

The critic is an effect-less independent reviewer, not a second workspace owner.

Risk remains governed by deterministic policy and factual post-draft evidence.

Model/Skill/workflow policy evolves only through reproducible evaluation and controlled rollout.

This patch therefore extends the existing Modbit architecture without creating another subsystem: the Model Gateway remains the inference boundary; the Agent Runtime remains the executor; Context, Prompt/Skill, policy, worktrees, checkpoints, verification and recovery remain canonical; and the new Execution Policy Router becomes the bounded decision layer that chooses **how** those existing components should be used for each request.

---

## 28. Adoption checklist

- [ ] Add this patch to the active dossier.
- [ ] Mark the frontier-only reasoning ADR superseded.
- [ ] Mark the frontier-token-only efficiency ADR superseded.
- [ ] Add ADR-R-039 through ADR-R-048.
- [ ] Reconcile `09-Agent-Runtime-and-Model-Gateway.md`.
- [ ] Reconcile `12-Evidence-Evaluation-and-Robustness.md`.
- [ ] Extend `14-Algorithms.md`.
- [ ] Add EPR work packages to `04-Task-Breakdown.md`.
- [ ] Update `13-Capability-Matrix-and-Migration.md`.
- [ ] Preserve all non-conflicting current runtime/security/context/verification decisions.
- [ ] Establish current DIRECT baseline before enabling adaptive workflow execution.
- [ ] Require holdout evidence before first CASCADE/CRITIQUE promotion.
- [ ] Require rollback-ready policy versions before production rollout.
