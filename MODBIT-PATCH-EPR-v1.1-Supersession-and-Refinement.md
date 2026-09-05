# Modbit Dossier Patch — EPR v1.1 Supersession and Refinement

**Patch ID:** MODBIT-PATCH-EPR-2026-09-05-v1.1  
**Status:** READY FOR ADOPTION  
**Applies to:** `MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md` v1.0  
**Change type:** Surgical supersession / refinement  
**Authority:** This patch supersedes only the conflicting clauses identified below. All non-conflicting v1.0 content remains active.

---

## 1. Purpose

The v1.0 Execution Policy Router patch remains the correct architectural direction.

This v1.1 patch simplifies and strengthens it by making five changes:

1. Replace distinct DIRECT/CASCADE/CRITIQUE runtime templates with **one bounded conditional transaction plan**.
2. Replace mean expected quality with a **confidence-adjusted quality floor**.
3. Move escalation and critic performance statistics out of the Model Registry into a separately versioned **Outcome Statistics Store**.
4. Split post-draft evaluation into **Realized Risk** and **Acceptance** gates.
5. Replace the imprecise term "effect-less critic" with an **Isolated Non-Committing Reviewer** whose sandbox permits bounded evidence gathering but no persistent or external effects.

No new orchestration subsystem is introduced.

---

## 2. Normative architecture after v1.1

```text
REQUEST
   |
   v
REQUEST PROFILER
   |
   | intrinsic demand
   | p_floor_success
   | confidence
   | OOD
   v
POLICY ENVELOPE
   |
   | legal models/providers
   | permitted effects
   | minimum assurance
   | cost/latency budget
   v
PLAN COMPILER
   |
   | Model Registry
   | Skill Registry
   | Outcome Statistics
   | cache/session state
   | current prices/latency/health
   v
CHEAPEST FEASIBLE CONDITIONAL PLAN
whose confidence-adjusted quality clears the selected floor
   |
   v
BOUNDED TRANSACTION EXECUTOR
   |
   +-- initial solver leg
   |
   v
STATIC EVIDENCE
   |
   +-- build/typecheck/lint
   +-- targeted tests
   +-- scope/API/dependency checks
   +-- security/diff signals
   |
   v
REALIZED RISK EVALUATOR
   |
   v
REQUIRED ASSURANCE
   |
   v
ACCEPTANCE GATE
   |
   +-- ACCEPT ---------> atomic commit
   |
   +-- ESCALATE -------> activate prevalidated stronger-solver continuation
   |
   +-- REVIEW ---------> activate prevalidated isolated reviewer continuation
   |                         |
   |                         v
   |                      revise
   |
   +-- HUMAN ----------> approval / intervention
```

All continuation branches MUST be validated and budgeted before the transaction starts.

The executor does not invent new workflow branches at runtime.

---

## 3. Supersession: workflow templates

The v1.0 language treating DIRECT/CASCADE/CRITIQUE as distinct primary runtime templates is superseded.

The canonical planning object is a **ConditionalExecutionPlan** containing:
- one initial execution leg;
- deterministic/static evidence requirements;
- realized-risk policy;
- acceptance requirements;
- zero or more prevalidated continuation slots;
- a worst-case bounded budget.

Example:

```yaml
ConditionalExecutionPlan:
  initial_leg:
    role: solver
    model_config_ref: economical_floor

  evidence_profile:
    - typecheck
    - targeted_tests
    - scope_check

  continuations:
    on_quality_reject:
      leg:
        role: escalation
        model_config_ref: frontier_solver

    on_review_required:
      legs:
        - role: reviewer
          model_config_ref: reviewer_B
        - role: reviser
          model_config_ref: original_solver

    on_human_required:
      action: wait_for_approval

    on_accept:
      action: atomic_commit
```

Retain DIRECT/CASCADE/CRITIQUE only as derived telemetry/outcome labels:

```text
initial -> commit
= DIRECT outcome

initial -> gate reject -> stronger solver
= CASCADE outcome

initial -> review -> revise
= CRITIQUE outcome
```

They are not separate orchestration engines.

---

## 4. Supersession: quality objective

Replace soft quality-minus-cost optimization as the primary production rule with:

```text
choose the lowest-cost eligible plan
subject to confidence-adjusted verified quality >= quality_floor(mode)
```

Formally:

```text
minimize:
    E[cost(plan)]

subject to:
    LCB(Q(plan | request)) >= tau(mode)
    worst_case_cost(plan) <= request_budget
    policy(plan) == allowed
    required_assurance(plan) is satisfiable
```

`LCB` is a lower-confidence bound or equivalent conservative posterior criterion.

Equivalent:

```text
P(Q(plan) >= tau) >= 1 - delta
```

Modes set quality floors and confidence requirements, not merely scalar weights.

If no eligible plan clears the quality floor:
1. choose the highest-confidence/highest-quality eligible plan within hard policy limits;
2. mark `QUALITY_FLOOR_INFEASIBLE`;
3. surface the condition in telemetry;
4. do not silently pretend the target was met.

---

## 5. Cold-start rule

Published and internal benchmarks may seed priors, but they MUST be treated as low-confidence until representative product evidence accumulates.

A high mean with a weak lower-confidence bound does not qualify a cheap plan.

**Cheap routing is earned by evidence.**

---

## 6. Model Registry vs Outcome Statistics

The Model Registry holds relatively stable/current properties:

```yaml
ModelProfile:
  model_id: string
  revision: string
  provider_id: string
  family: string
  roles: []
  capabilities: {}
  context_limits: {}
  tool_compatibility: {}
  vision_capability: optional
  current_cost: {}
  latency: {}
  health: {}
  availability: {}
  governance: {}
```

Empirical workflow behavior moves to a separately versioned Outcome Statistics Store:

```yaml
OutcomeStatistics:
  stats_version: string

  solver_success:
    key:
      demand_bin
      model_config
      skill_set
      harness_version
    value:
      mean
      confidence_interval
      sample_count

  escalation_probability:
    key:
      demand_bin
      solver_config
      skill_set
      gate_version
      repo_slice
    value:
      probability
      confidence_interval
      sample_count

  critic_value:
    key:
      solver_family
      critic_config
      task_slice
    value:
      defect_recall
      false_positive_rate
      critical_defect_recall
      confidence_interval
      sample_count

  revision_success:
    key:
      solver_config
      critic_config
      finding_class
    value:
      probability
      confidence_interval
      sample_count
```

Compiler inputs become:

```text
RequestProfile
+ PolicyEnvelope
+ Model Registry
+ Skill Registry
+ Outcome Statistics
+ Cache/Session State
+ Current Economics/Health
-> Conditional Plan Compiler
```

---

## 7. Supersession: escalation probability

Do not treat `P(escalate | demand, floor_model)` as a model property.

Use:

```text
P(escalate |
  demand,
  solver_config,
  skill_set,
  gate_version,
  repository_slice,
  verification_availability)
```

Escalation is an interaction among solver, task and gate.

---

## 8. Supersession: risk handling

The Policy Envelope defines:
- legal plan/model/provider set;
- permitted effect classes;
- protected paths/surfaces;
- minimum verification profile;
- mandatory review conditions;
- mandatory human-approval conditions;
- secret/network/deploy rules;
- request budget.

It does not need a generic learned risk score.

After a candidate exists, derive factual realized risk:

```yaml
RealizedRisk:
  level: LOW | MEDIUM | HIGH | CRITICAL

  reasons:
    - protected_path
    - auth_surface
    - authorization_change
    - secret_handling
    - dependency_change
    - lockfile_change
    - ci_cd_change
    - infrastructure_change
    - migration
    - public_api_change
    - broad_blast_radius
    - sparse_test_coverage
    - unexpected_scope_expansion
    - external_effect_request

  minimum_assurance:
    FAST | STANDARD | GOVERNED | HIGH_ASSURANCE

  independent_review_required: boolean
  human_required: boolean
```

Post-draft evaluation may strengthen assurance requirements. It MUST NOT weaken policy minima.

---

## 9. Realized Risk vs Acceptance Gate

Realized Risk answers:

> How much assurance does this candidate require?

Acceptance Gate answers:

> Does the current evidence satisfy that assurance requirement?

```yaml
AcceptanceGateResult:
  required_assurance: string

  evidence:
    build: optional
    typecheck: optional
    lint: optional
    tests: optional
    security: optional
    api_compatibility: optional
    reviewer: optional

  verdict:
    ACCEPT | REJECT | INCONCLUSIVE

  missing_evidence: []
  evidence_refs: []
```

Never collapse safety/risk and correctness into one classifier.

---

## 10. Reviewer terminology and capability model

Replace "effect-less critic" with:

> **Isolated Non-Committing Reviewer**

Canonical capability contract:

```text
canonical tracked tree:
  READ ONLY

throwaway review worktree:
  EPHEMERAL WRITE ALLOWED

process execution:
  SANDBOXED

network:
  DENY BY DEFAULT

secrets:
  DENY BY DEFAULT

canonical workspace mutation:
  DENY

git commit/push:
  DENY

deploy/external actions:
  DENY

persistent effects:
  NONE
```

The reviewer SHOULD receive:
- original task;
- acceptance criteria;
- candidate diff;
- relevant source/context;
- static verification results;
- diagnostics;
- targeted test results;
- dependency/API/security evidence.

The reviewer SHOULD NOT receive the solver's hidden reasoning trace.

Independent judgment means context isolation from solver reasoning, not ignorance of the codebase.

---

## 11. Evidence-first review ordering

Default order:

```text
candidate
   |
   v
deterministic/static checks
   |
   v
realized-risk evaluation
   |
   v
acceptance assessment
   |
   +-- sufficient -> commit
   +-- review needed -> isolated reviewer
   +-- solver inadequate -> escalation
   +-- policy requires -> human
```

Do not invoke a reviewer merely to rediscover deterministic compiler/type/test failures.

Reviewer tool cost and latency MUST be included in plan pricing.

---

## 12. Prevalidated continuation slots

Runtime continuation is allowed only when compiled in advance.

Example:

```yaml
AllowedContinuations:
  QUALITY_REJECT:
    plan_ref: stronger_solver_continuation

  REVIEW_REQUIRED:
    plan_ref: reviewer_then_revision

  HUMAN_REQUIRED:
    plan_ref: wait_for_human

  PROVIDER_FAILURE:
    plan_ref: validated_provider_fallback
```

At compile time validate:
- bound models exist;
- roles are eligible;
- reviewer isolation is enforceable;
- fallback chains terminate;
- worst-case total cost fits budget;
- timeouts/retries are bounded;
- context requirements fit;
- provider/data-policy constraints hold.

At runtime, gates activate already compiled continuations; they do not synthesize new topologies.

---

## 13. Multi-turn routing refinement

Express stickiness as economics.

```text
switch_cost =
    lost_cache_value
  + re_prefill_cost
  + cache_write_cost
  + expected_switch_latency
  + hysteresis_penalty
```

A switch occurs only when the feasible alternative remains better after switch cost.

Preferred re-evaluation boundaries:
- new task;
- compaction/context epoch;
- significant demand drift;
- provider degradation;
- quality rejection/escalation;
- task-mode transition;
- explicit objective-mode change.

---

## 14. Logging changes

Each request MUST retain enough state for exact policy replay:

```yaml
RoutingDecisionRecord:
  request_id: string

  request_profile:
    version: string
    ref: string

  policy_envelope:
    version: string
    ref: string

  model_registry_version: string
  skill_registry_version: string
  outcome_statistics_version: string

  compiler_version: string
  gate_version: string
  realized_risk_version: string

  candidate_plans:
    - plan_id
    - predicted_quality_mean
    - predicted_quality_lcb
    - predicted_cost
    - predicted_latency
    - eligibility
    - exclusion_reasons

  chosen_plan_id: string

  executed_path:
    - leg_id
    - role
    - outcome

  final_outcome: string
```

---

## 15. Reward attribution

Keep three levels:

### Request-level
- final verified success;
- user satisfaction;
- keep/revert;
- human intervention;
- total cost;
- wall-clock;
- repair turns.

### Leg-level
- initial solver accepted/rejected;
- escalation success;
- reviewer genuine-defect discovery;
- reviewer false positive;
- revision success;
- tool reliability.

### Gate-level
- accepted result later shown wrong;
- rejected result later shown valid;
- realized-risk miss;
- realized-risk over-classification.

A successful frontier escalation MUST NOT be attributed as success to a failed economical first leg.

---

## 16. Gate evaluation becomes release-critical

Track independently:

```text
acceptance_false_accept_rate
acceptance_false_reject_rate
realized_risk_false_negative_rate
realized_risk_false_positive_rate
critical_surface_miss_rate
```

Router gains cannot excuse degraded gate safety.

---

## 17. Updated policy-search space

Search conditional-plan parameters, not arbitrary graphs:

- initial solver;
- solver reasoning effort;
- Skill set;
- evidence profile;
- acceptance thresholds;
- realized-risk thresholds;
- escalation solver;
- reviewer;
- reviewer tool budget;
- revision solver;
- continuation thresholds;
- leg token budgets;
- timeouts;
- retry ceilings;
- context tier.

### Stage A
Reach confidence-adjusted quality feasibility.

### Stage B
Minimize total cost, latency, intervention and recovery burden along the feasible boundary.

---

## 18. Required amendments to v1.0

Apply these substitutions:

1. Replace DIRECT/CASCADE/CRITIQUE as primary runtime templates with `ConditionalExecutionPlan`.
2. Keep those three names only as derived path labels.
3. Replace soft expected utility with confidence-adjusted quality-floor feasibility.
4. Move empirical escalation/critic/revision statistics out of Model Registry.
5. Split Realized Risk from Acceptance Gate.
6. Replace "effect-less critic" with Isolated Non-Committing Reviewer.
7. Require all continuation paths to be prevalidated and worst-case budgeted.
8. Jointly tune initial leg, evidence profile, gates and continuation parameters offline.

---

## 19. ADR amendments

Add after the existing EPR ADRs:

```text
ADR-R-049 LOCKED
DIRECT, CASCADE and CRITIQUE are execution-path classifications, not separate
orchestration engines. The canonical runtime object is a bounded conditional
transaction with prevalidated continuation slots.

ADR-R-050 LOCKED
Plan eligibility uses a confidence-adjusted verified-quality floor. Cheap plans
are not eligible solely because their point-estimate quality exceeds the floor.

ADR-R-051 LOCKED
Empirical solver success, escalation probability, reviewer value and revision
success live in a separately versioned Outcome Statistics Store rather than the
Model Registry.

ADR-R-052 LOCKED
Realized risk determines required assurance; the Acceptance Gate determines
whether available evidence satisfies that assurance. Risk and correctness are
not one classifier.

ADR-R-053 LOCKED
Independent review executes as an Isolated Non-Committing Reviewer: read-only on
canonical tracked state, ephemeral writes and bounded execution only in a
throwaway review environment, no persistent or external effects.

ADR-R-054 LOCKED
Runtime escalation/review/human branches may activate only precompiled,
prevalidated continuation slots whose worst-case budget and policy eligibility
were checked before execution.

ADR-R-055 LOCKED
Request, leg and gate outcomes are logged separately so successful escalation
does not misattribute success to a failed earlier leg.

ADR-R-056 LOCKED
Gate calibration is release-critical and evaluated independently of router
quality. Router improvements cannot excuse an unacceptable false-accept or
critical-risk-miss rate.
```

If these ADR numbers are already occupied, allocate the next available IDs and preserve the semantic order.

---

## 20. Task delta

Add only if not already represented:

### EPR-014 — Conditional plan migration
Refactor workflow-template authority into `ConditionalExecutionPlan`.

### EPR-015 — Outcome Statistics Store
Implement versioned empirical statistics separate from model configuration.

### EPR-016 — Confidence-adjusted feasibility
Implement LCB/posterior-based plan eligibility.

### EPR-017 — Realized Risk / Acceptance split
Separate assurance classification from evidence acceptance.

### EPR-018 — Isolated Non-Committing Reviewer
Implement bounded review sandbox with no persistent/external effects.

### EPR-019 — Gate calibration suite
Create independent realized-risk and acceptance-gate benchmark suites.

Do not duplicate equivalent existing tasks.

---

## 21. Integration prompt

```text
GOAL: Upgrade the already-adopted Modbit Execution Policy Router patch to EPR v1.1 without duplicating architecture, tasks, ADRs or runtime subsystems.

1. Read the current active Modbit dossier, the adopted EPR v1.0 patch, decision register, Agent Runtime/Model Gateway spec, Evidence/Evaluation spec, Algorithms, Task Breakdown and Capability Matrix.
2. Apply EPR v1.1 as a surgical supersession. Preserve all non-conflicting v1.0 content.
3. Replace DIRECT/CASCADE/CRITIQUE as separate runtime templates with one ConditionalExecutionPlan containing an initial leg plus prevalidated continuation slots. Keep DIRECT/CASCADE/CRITIQUE only as derived telemetry/outcome labels.
4. Replace soft quality-minus-cost routing with: choose the cheapest eligible plan whose confidence-adjusted verified-quality lower bound clears the selected mode's quality floor. If none clears it, use the best eligible fallback and record QUALITY_FLOOR_INFEASIBLE.
5. Keep the Request Profiler pure. Do not add model, price, availability, org policy or cache state as intrinsic learned features.
6. Separate Model Registry from Outcome Statistics. Move solver success, escalation probability, reviewer value and revision success to versioned empirical statistics with sample counts/confidence.
7. Split post-draft logic into Realized Risk (required assurance) and Acceptance Gate (whether evidence satisfies assurance). Risk cannot weaken policy minima.
8. Replace "effect-less critic" with Isolated Non-Committing Reviewer: canonical tree read-only; bounded exec/ephemeral writes only in disposable worktree; network/secrets deny by default; no persistent/external effects; no solver hidden reasoning in reviewer context.
9. Permit escalation/review/human actions only through continuation slots validated and budgeted before execution.
10. Add ADR-R-049..056 and EPR-014..019, but reconcile IDs if the repository has since allocated them. Do not create duplicates.
11. Update algorithms, schemas, evals and telemetry accordingly. Preserve Model Gateway, Run/Session, Skills, Context, capability/policy, worktree/checkpoint, verification and recovery authority.
12. Run dossier integrity/reconciliation checks and report: files changed, superseded clauses, task/ADR collisions resolved, remaining conflicts, and exact acceptance gates required before implementation.
```

---

## 22. Final authority statement

After adoption of v1.1:

> **Modbit profiles intrinsic task demand, compiles the cheapest policy-compliant bounded conditional transaction whose confidence-adjusted expected verified quality clears the selected quality floor, executes an initial solver leg, derives assurance requirements from factual post-draft risk, and activates only prevalidated continuation legs when evidence requires stronger solving, independent review or human intervention.**

The key separation is:

```text
Predictor
= what does the request intrinsically require?

Policy Envelope
= what is legally/permissibly executable?

Compiler
= which bounded conditional plan is cheapest while clearing quality?

Realized Risk
= how much assurance does the actual candidate require?

Acceptance Gate
= has that assurance requirement been met?

Executor
= perform only the already validated transaction/continuations.

Outcome Statistics
= what have we empirically learned about configurations?

Policy Lab
= improve future plan/gate parameters offline.
```

This is the normative v1.1 refinement of the existing Execution Policy Router patch.
