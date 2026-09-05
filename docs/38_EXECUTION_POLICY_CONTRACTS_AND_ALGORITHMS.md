# Execution policy contracts and algorithms

> **Authority:** DR-EPR-2026-09-05-v1.1, ADR-R-039..056. **Implementation:** NOT_STARTED.  
> This specification refines the illustrative contracts in `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`. Implement behind the existing owners in `12_REPOSITORY_AND_MODULE_LAYOUT.md`; do not create a parallel execution engine.

## Shared serialization and validation

Every persisted routing contract has `schema_version`, immutable record ID, tenant/session/task/run identity as applicable, created timestamp, content digest and provenance references. A digest is not an authorization token. Required versions cannot be empty. References resolve under the caller's tenant and retention policy. Unknown incompatible major versions fail closed before execution; additive fields follow `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`.

Money is a checked nonnegative integer in a named currency and scale; cross-currency comparisons require a pinned conversion snapshot. No floating-point budget arithmetic. Probabilities are finite numbers in [0,1]. Token counts, retries and durations are checked bounded integers; timeout and output ceilings are positive, retry ceilings may be zero. Reject NaN, infinity, integer overflow, duplicate legs, missing references and ambiguous enum values. Prices have effective time, revision and conservative fallback when stale. Missing usage is unknown, never zero cost.

| Contract | Required refinement beyond the architecture example |
|---|---|
| PolicyEnvelope | policy snapshot version/digest, derived-at and expiry, canonical entitlement/risk source refs, remaining daily/request reservation, allowed conditional slot semantics, manual-pin, mode quality floor and confidence constraints, no raw credentials |
| RequestProfile | schema/profiler/feature-extractor/calibration versions, immutable request/scope feature digest, qualification-cohort ref for p_floor_success, bounded feature freshness and fallback reason |
| ModelProfile | registry generation/signature/digest/freshness, exact model/config revision, roles, context/output/tool/media support, price/health observation snapshots; no solver/escalation/reviewer/revision empirical statistics |
| ConditionalExecutionPlan | input snapshot digest, policy/profiler/registry/skill/statistics/compiler/gate/realized-risk versions, baseline and candidate revisions, routing epoch, Run lease generation, initial leg plus prevalidated acyclic continuation slots, named terminal states, finite max_total_attempts/max_revisions, verification reserve, fallback bounds |
| RoutingDecisionRecord / RoutingTrace | all candidate quality means/LCBs/costs/latencies/eligibility/exclusion reasons, chosen plan, QUALITY_FLOOR_INFEASIBLE flag when required, ordered executed slot/leg path, requested/resolved configuration, deterministic choice probability 1 or actual exploration propensity, route epoch, every invocation attempt including failed/retried/fallback legs, cache units, accounting completeness |
| AcceptanceGateResult | gate/version/leg/attempt, exact candidate revision and acceptance contract digest, checks required/run/skipped with reason, ACCEPT/REJECT/INCONCLUSIVE, required_assurance, missing_evidence, evidence refs and separate RealizedRisk ref; confidence alone cannot accept |
| ReviewerResult | reviewer/solver configuration and family, plan/leg/attempt/candidate revision, structured verdict/findings/evidence scopes; findings are untrusted until resolved and validated |
| OutcomeRecord | separate request/leg/gate outcomes, no credit to a failed initial leg after successful escalation, plan/provenance refs, final pass/fail/partial/cancelled, verified/first-pass success, raw feedback with attribution window/source/time, nullable missing signals, complete inference/verification cost and human intervention, reward version |
| RealizedRisk | factual revision-bound reasons, LOW/MEDIUM/HIGH/CRITICAL, minimum_assurance, independent_review_required, human_required, rule version and evidence refs; cannot weaken policy minima |
| OutcomeStatistics | stats_version and source digests, means/confidence intervals/sample counts for solver success, escalation, reviewer value and revision success; joint keys per doc 27, independently versioned from Model Registry |
| RoutingSessionState | existing session projection with active model/workflow, cache ref, last profile, last route time, route epoch and generation; no separate session store |

Plans retain the original request. ConditionalExecutionPlan schema 2 is the only new-write executable plan. Each slot binds ID, trigger/predecessor, allowable activation count, fully resolved legs/configs, role/context/tool/process/timeout/retry budgets and terminal conditions before initial dispatch. Total attempts and reachable sequential branches are bounded over the whole request, not merely per slot. Manual pin conflicts cannot silently switch model or weaken assurance.

Legacy ExecutionPlan/QualityGateResult/CriticResult are explicit decode adapters only. A lossless legacy template may migrate after validation of every slot, budget and current authority; absent LCB/stats/provenance is recorded unknown/infeasible, never invented. Unknown major or unmappable pending state blocks mutation and preserves export/recovery. Old DIRECT/CASCADE/CRITIQUE labels never authorize executable branches or capability grants.

## Conditional transaction grammar

Normative pseudocode (not executable product code):

```text
compile + reserve entire bounded transaction
  -> initial solver
  -> static/deterministic evidence
  -> RealizedRisk(required assurance)
  -> AcceptanceGate(ACCEPT | REJECT | INCONCLUSIVE)
  -> atomic apply OR activate an existing slot:
       QUALITY_REJECT  -> stronger solver -> evidence/risk/acceptance
       REVIEW_REQUIRED -> isolated reviewer -> bounded revision -> evidence/risk/acceptance
       HUMAN_REQUIRED  -> existing approval/intervention wait
       PROVIDER_FAILURE -> validated replacement leg
```

Compile combinations/order when multiple obligations can occur: review followed by escalation cannot be priced as mutually exclusive if both are reachable. The default at most one revision round survives; any required re-review is compiled and budgeted. No recursive review or generated DAG. Human waits have finite cancellation/timeout and bind canonical approval scope; a reply never weakens other checks. Acceptance atomic apply is not authority for Git commit/push.

Runtime activates slots only. If new policy/risk/provider conditions need a missing topology, emit REQUIRED_CONTINUATION_UNAVAILABLE and stop/reconcile the transaction. A separately admitted new transaction may compile at a safe boundary with new epoch and remaining request budget; it cannot be smuggled into the old plan as an unbounded fallback. Preserve original request, prior candidate/failure evidence, lineage and complete accounting.

## RequestProfiler

1. Snapshot request/mode/scope, bounded repository summary, diagnostics/test presence and permitted same-Run history features from Context Engine. No full repository scan on the routing path.
2. Remove model/provider identities, prices, health, cache state, allowlists and objective weights from intrinsic features. Retain provenance and feature-extractor version.
3. Run the pinned deterministic or small learned classifier; compute task/domain/modifiers, capability demand, calibrated p_floor_success, confidence and OOD score.
4. On timeout/unavailable/stale features emit a deterministic conservative fallback profile with explicit low confidence and reason. Never invoke a general LLM as profiler fallback.
5. Persist the profile and calibration cohort reference. Target p50 <25 ms where practical and p95 <50 ms on the declared routing hardware; include feature extraction in the measurement.

## EnumerateEligiblePlans

1. Derive the legal PolicyEnvelope and freeze profile, registry, Skill, Outcome Statistics, economics/health, cache/session, budget and verification snapshots with their versions.
2. Apply hard tenant/provider/model/residency/retention/role/context/modality/availability/manual-pin/assurance/isolation constraints first. A required unavailable review sandbox removes that slot/plan; a model role alone is not empirical qualification.
3. Enumerate finite conditional parameters: initial solver, evidence profile, factual risk policy, acceptance requirements and prevalidated escalation/review/revision/human/provider slots. The compiler never enumerates DIRECT/CASCADE/CRITIQUE as separate runtime engines or accepts a model-generated topology.
4. Validate reachability, maximum activations/retries, termination and worst-case inference plus verification/reviewer-tool/process/switch cost against remaining request and daily cap. Store exclusion reasons before cost ranking.
5. If no hard-eligible plan remains, fail explicitly. Otherwise pass the hard-eligible set to confidence feasibility. Missing statistics do not imply zero failure probability or qualification.

## CompileConfidenceFeasiblePlan

1. Join intrinsic demand with separately versioned Outcome Statistics for exact model/Skill/effort/context/harness/gate/repository/verification slices. Estimate quality mean, conservative LCB or approved posterior criterion, uncertainty and sample count for the entire conditional transaction.
2. Published/internal benchmark means seed low-confidence priors. Cold start, sparse samples, distribution shift or stale statistics cannot qualify a cheap plan on its point estimate. Pin confidence method, calibration, tau(mode), delta(mode) and minimum representative evidence in the threshold profile.
3. Retain plans satisfying LCB(Q) >= tau (or equivalent P(Q >= tau) >= 1-delta) and every hard constraint. Choose minimum expected complete cost among these plans. Modes govern quality/confidence, not a soft price for violating quality. Use declared latency constraints and stable digest tie-breaks.
4. Escalation probability conditions on demand, solver config, Skill set, gate version, repository slice and verification availability. Include initial/evidence/reviewer-tool/revision/escalation/fallback/cancellation/recovery costs and all allowed sequential combinations. Preserve separate units or versioned conversions for latency/hysteresis.
5. If none clears quality, rank hard-eligible alternatives by the pinned conservative confidence/quality criterion; choose best within policy/budget and mark QUALITY_FLOOR_INFEASIBLE. Record each mean/LCB, exclusion, expected cost/latency and fallback reason. Never claim the quality target was met or waive actual acceptance/assurance.
6. Compare staying/switching only among eligible alternatives including lost cache, refill/cache-write, expected switching latency and hysteresis. Maintain route epoch and switch reason. Deterministic inputs yield deterministic output; allowed randomized exploration happens only over eligible plans with persisted seed/propensity, outside critical/high-assurance traffic unless explicitly approved.

## ValidateConditionalExecutionPlan

1. Resolve every initial and slot reference; verify tenant, signatures, versions, statistics compatibility/freshness, base revision, routing epoch and Core lease. Every required version is nonempty; unknown schemas or missing refs fail before dispatch.
2. Validate initial solver and all continuation model/role/context/capability/privacy constraints. Plans only narrow policy. Prevalidate triggers, predecessor states, maximum activations, finite total retries/revisions/timeouts, terminating provider fallback and human wait scope.
3. Enforce Isolated Non-Committing Reviewer: canonical tree read-only, ephemeral writes/processes only in disposable isolated worktree, bounded CPU/time/output/disk/process count; network/secrets deny by default; canonical mutation/commit/push/deploy/persistent/external actions denied. No solver hidden reasoning in default review context.
4. Atomically reserve worst-case reachable transaction cost, including mandatory checks, reviewer tools, switches, all sequential slots, price uncertainty and in-flight request/daily commitments. Mutual-exclusion savings are allowed only when proved by the compiled transitions.
5. Verify every acceptance terminal satisfies current-revision assurance/checks/approval/findings and atomic compare-and-apply. Persist conditional plan, slot table, validation digest, reservation and Run activation in the existing event/projection transaction.
6. Runtime revalidates revocation, lease, epoch, revision and current policy before each dispatch/effect. It may deny an old slot but cannot add one. Missing new obligations cause safe stop/reconciliation and a separate admitted transaction if needed.

## ExecuteConditionalTransaction

1. Load the admitted immutable plan under Core lease; compile role-specific context/prompt/Skill/tool manifest for initial leg. Record attempt/usage reservation before Gateway dispatch; invoke bounded provider and route requested tools through normal capability/effect checks.
2. Persist exact candidate revision and deterministic/static results before review selection. DeriveRealizedRisk separately, then EvaluateAcceptanceGate. Do not use the reviewer to rediscover a known compiler/type/test failure.
3. On ACCEPT, verify all obligations and current revision once more, then use the existing Change Engine for atomic application. An infeasible predicted quality floor remains explicitly flagged even when actual evidence permits acceptance; prediction is not a completion proof.
4. Otherwise ActivateContinuation for compiled QUALITY_REJECT, REVIEW_REQUIRED, HUMAN_REQUIRED or PROVIDER_FAILURE. Trigger precedence and any combinations are validated upfront. An absent/revoked/exhausted required slot yields explicit failure/attention; no synthesized topology.
5. After every revised/escalated candidate rerun current-revision static evidence, risk and acceptance. All attempts, review tool use and failed initial results stay in accounting. Exhaustion/cancellation produces exact partial state without partially accepted patch.
6. Derive DIRECT from initial-to-accept, CASCADE from actual escalation, CRITIQUE from actual review/revision. Ordered executed path is canonical when several labels apply; no label drives dispatch.

## ActivateContinuation

1. Match trigger to an existing slot at the exact expected predecessor/plan/epoch/revision. Check pending obligations, permitted count, current binding eligibility and remaining reserved resources.
2. Commit slot activation with an idempotency key tied to plan/slot/activation ordinal and the existing event/protocol projection. A replay or stale result cannot activate a second copy.
3. Bind continuation capabilities/context to the prevalidated ceiling and current policy. Escalation receives original task, candidate/checkpoint choice and evidence of failure; continuing from candidate versus restored base must be explicit and revision-correct.
4. Reviewer activation uses ExecuteIsolatedReview. Human activation uses ordinary durable approval/intervention state; provider fallback reconciles ambiguous tool/effect outcomes before dispatch. Unknown effects are never retried just because an earlier model failed.
5. No slot receives a new budget: request cap minus spent, in-flight reservations and mandatory verification reserve bounds its allowance. Cancellation/restart reconcile charges/effects and preserve activation count before resuming.

## ExecuteIsolatedReview

1. Freeze current candidate diff, acceptance criteria, scoped source, static tests/diagnostics and API/dependency/security evidence. Exclude solver hidden reasoning channels by default; independent judgment still receives relevant codebase facts.
2. Through existing worktree/execution/capability owners create a disposable review environment with read-only canonical mounts, ephemeral scratch writes, bounded sandboxed process execution and deny-default network/secrets. Do not expose Gateway credentials or mount credential stores. Sandbox unavailability blocks admission; prompts alone do not prove isolation.
3. Allow real bounded evidence gathering (such as targeted tests creating scratch output) and reject canonical writes, path/symlink escapes, persistent effects, commits/push/deploy and external actions at the real effector. Price inference/tool/process usage in the same transaction reservation.
4. Validate ReviewerResult finding refs, schema and exact candidate revision. Findings remain untrusted until grounded. Reviser uses normal task permissions on its candidate workspace; never apply review scratch changes to canonical state. Required re-review uses only admitted bounded slots.
5. On completion/cancel/timeout/crash kill the review process tree, revoke capabilities and dispose scratch storage; trusted Core may first collect bounded immutable evidence through the existing artifact owner. No reviewer persistent-write capability exists. Evidence versions/candidate binding survive cleanup and restart.

## DeriveRealizedRisk

1. Inspect factual candidate paths/diff, protected/auth/authorization/secret/CI/infrastructure/dependency/lockfile/schema/API changes, blast radius, coverage gaps, unexpected scope and requested effects under revision lock.
2. Canonical policy derives LOW/MEDIUM/HIGH/CRITICAL, reasons, minimum_assurance, independent_review_required and human_required with rule version and facts. Retain the stricter of policy minima and factual requirements; no learned scalar can downgrade assurance.
3. Persist RealizedRisk separately from test correctness and gate verdict. Passing tests do not erase a critical surface or missing human approval. Send required assurance to the Acceptance Gate; unavailable obligations cannot cause implicit acceptance.

## EvaluateAcceptanceGate

1. Take PolicyEnvelope minima plus current RealizedRisk and acceptance criteria as required assurance. Gather current-revision build/typecheck/lint/tests/security/API/reviewer/approval evidence through the normal verification owner.
2. Evaluate evidence sufficiency independently of risk classification: ACCEPT only when all required valid evidence exists; explicit failed evidence gives REJECT; missing/stale/timed-out mandatory evidence gives INCONCLUSIVE. Record missing_evidence, evidence refs and exact gate/risk versions.
3. A measured semantic judge may supplement insufficient deterministic oracles but cannot override a hard failure or grant capabilities. Gate confidence is not permission. Core maps verdict plus pending obligations to precompiled slots only.
4. Record acceptance false accepts/rejects separately from realized-risk false negatives/positives and critical_surface_miss_rate. Later corrections link exact candidate and versions. Release thresholds apply independently of router quality or savings.

## MaterializeOutcomeStatistics

1. Consume versioned request, leg and gate observations from the existing canonical event/usage/outcome stores. Retain source digests, tenant/retention scope, timestamps, sample identity and attribution validity.
2. Aggregate solver success, joint escalation probability, relational reviewer value and revision success under exact doc 27 keys; keep mean/probability, confidence interval and sample_count. A successful escalation does not make the failed first leg a success. Do not join incompatible harness/gate/Skill/repository/verification slices or count repeated events twice.
3. Publish an immutable stats_version and calibration/data-partition digest using existing artifact/store owners. Registry changes do not mutate statistics in place; compiler decisions pin both versions. Priors have explicit source and low-confidence label, not fabricated representative samples.
4. Validate compatibility/freshness/privacy before compiler use. Missing or invalid statistics invoke conservative prior/infeasible behavior, not silent cheap qualification. This derived dataset is rebuildable from retained permissible evidence and is not a new event/protocol/memory authority.

## CompleteAccountingAndAttribution

1. Reconcile all invocation attempts keyed by request/plan/leg/attempt/provider request ID; include retries, cancellations, failed initial solver, reviewer inference/tools/processes, revision, fallback and escalation.
2. Deduplicate cumulative versus incremental usage reports by adapter semantics. Record total input and cached subset without double charging, uncached input, output, cache read/write and observed or conservatively estimated amounts.
3. Settle completed reservations exactly once. Preserve uncertainty and reserve for in-flight/unknown charges until reconciliation; never refund unknown work as free. Include deterministic verification cost, total wall-clock and raw human-intervention signals separately.
4. Append accounting/evidence events and OutcomeRecord transactionally, with distinct request, leg and gate observations. Preserve initial failure when escalation succeeds; link reviewer genuine-defect/false-positive and revision success separately from final request success. Label direct-frontier comparison estimated unless an isolated alternative was actually executed; observed replay still carries its environment identity and comparability limits.

## CounterfactualReplay

1. Admit only privacy/retention-approved trajectories; capture original request, immutable repository revision, policy/model/skill/statistics/gate/risk versions, original decision record and observed factual tool evidence.
2. Create a disposable scratch worktree with sanitized data, no production credentials, no production network/effect targets and a replay-only capability ceiling. Provider inference is dispatched by the existing Gateway service; no credentials reach replay code/context.
3. Validate alternative plan against this reduced envelope. Reuse recorded deterministic evidence only when its inputs/revision/oracle remain identical; otherwise recompute in the isolated environment or label the comparison unobservable.
4. Execute bounded alternatives offline. Record actual alternative outcomes/costs as observed counterfactuals and unexecuted model estimates as estimated. Never mirror production side effects for shadowing.

## PromoteRoutingPolicy

1. Search only the approved workflow/configuration space offline. First satisfy confidence-adjusted quality feasibility and independent acceptance/risk safety; then improve total verified-outcome economics on the feasible set.
2. Pin dataset partitions and thresholds before evaluation. Require training/replay → validation → untouched holdout → shadow → limited canary/A-B → production with named evidence and actual propensity logging for randomized assignment.
3. Reject missing evidence, quality-floor regression, safety failure or registry/statistics/schema incompatibility regardless of aggregate savings. Critical/high-assurance exploration requires explicit policy approval and stays budgeted.
4. Publish a signed immutable policy version with previous-good ref, registry compatibility, activation time and rollback thresholds. Atomically activate via privileged compare-and-swap; clients need no release.
5. Monitor calibration, workflow regret, independent acceptance/risk/critical-surface miss rates, reviewer recall/churn, recovery, cache economics and complete costs. Roll back at a safe boundary using current eligibility checks; in-flight effects remain governed by normal recovery. Neither telemetry nor Policy Lab becomes authorization authority.

## Recovery and typed failures

Persist conditional plan/slot activation/leg/attempt, budget spent/reserved, route epoch, candidate revision, gate/risk/reviewer refs and pending effects before advancing. Kill/restart tests must recover the exact state from events/protocol/checkpoints, never transcript reconstruction. Stale route, compaction or kernel generations reject late results. Cancellation settles usage and reconciles pending effects before a separately admitted transaction starts.

Stable error categories include PROFILE_UNAVAILABLE, REGISTRY_STALE, POLICY_CONFLICT, NO_ELIGIBLE_PLAN, PLAN_INVALID, BUDGET_EXHAUSTED, GATE_UNAVAILABLE, REVIEWER_REQUIRED_UNAVAILABLE, STALE_REVISION, STALE_EPOCH, PROVIDER_DEGRADED, UNKNOWN_EFFECT, QUALITY_FLOOR_INFEASIBLE and REQUIRED_CONTINUATION_UNAVAILABLE. Error projections show actionable reasons without secrets; retryability is determined by canonical policy and effect outcome, not the error text alone.
