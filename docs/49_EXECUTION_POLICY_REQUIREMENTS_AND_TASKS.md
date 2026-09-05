# Execution policy requirements and implementation tasks

> **Authority:** DR-EPR-2026-09-05-v1.1 (`06_EPR_V1_1_SUPERSESSION_DECISION.md`).  
> **Coverage:** 20 locked REQ-EPR/task/qualification triplets: EPR-000 baseline, 13 amended original packages and six distinct v1.1 delta slices. The 291 REQ-EV rows and existing canonical owners remain unchanged. All product work is NOT_STARTED; state/evidence live only in the graph.

Read active architecture doc 27, executable contracts/algorithms doc 38 and qualifications doc 61. DIRECT/CASCADE/CRITIQUE are derived path labels only. Existing task IDs are amended in place; no task instructs implementing obsolete templates first. ADR-R-049..056 supersede the clauses mapped in doc 06. New task numbers do not imply execution order; the dependency edges below do.

## Locked additive ledger

Source section references without a prefix refer to v1.0; `v1.1:` references the refinement patch. The shared parser reads the table. One task has one primary existing owner; dependencies include both these explicit prerequisites and the milestone's upstream gates.

| Requirement | Task | Qualification | Owner | Milestone | Phase | After | Patch sections | Mandatory behavior |
|---|---|---|---|---|---|---|---|---|
| REQ-EPR-000 | EPR-000 | QUAL-EPR-000 | model-gateway | M2 | 0 | M2.9 | 22.0,23.B | Keep the existing single-model engineering path working; instrument every invocation, cache unit, retry, verification and intervention; publish fixed-revision verified-outcome baseline before routing changes. |
| REQ-EPR-001 | EPR-001 | QUAL-EPR-001 | domain-events | M2 | 1 | EPR-000 | 5,6.2,7.2,8,9.6,9.8,10,11.2,16.1,21.001,v1.1:18 | Version PolicyEnvelope, RequestProfile, ModelProfile, ConditionalExecutionPlan schema 2, RoutingDecisionRecord/Trace, RealizedRisk, AcceptanceGateResult, ReviewerResult, OutcomeRecord and OutcomeStatistics refs; persist Run/slot/leg/attempt/epoch/budgets in existing stores; preserve explicit legacy decode provenance. |
| REQ-EPR-002 | EPR-002 | QUAL-EPR-002 | model-gateway | M2 | 1 | EPR-001 | 7.2,7.3,7.4,15,19.1,21.002,v1.1:18 | Keep model/provider/family/role, modality/context/tool compatibility, current economics/latency/health and governance in signed versioned Model Registry; remove empirical solver/escalation/reviewer/revision statistics to the separate versioned dataset interface; no desktop release for configuration updates. |
| REQ-EPR-003 | EPR-003 | QUAL-EPR-003 | model-gateway | M2 | 1 | EPR-002 | 6,21.003,23.C | Bound feature extraction; produce catalog-independent task/domain/modifier demand, p_floor_success, confidence and OOD with versioned calibration cohort. No general LLM hot-path call; deterministic conservative fallback; initially shadow only. |
| REQ-EPR-004 | EPR-004 | QUAL-EPR-004 | model-gateway | M2 | 1 | EPR-014,EPR-016 | 7,8,21.004,23.A,v1.1:18 | Enumerate promoted conditional parameters and prevalidate every initial/continuation binding, trigger, isolation, assurance and worst-case total budget before dispatch. Use confidence-feasibility from EPR-016, then lowest expected complete cost; no primary DIRECT/CASCADE/CRITIQUE template dispatch or runtime branch synthesis; manual pins retain policy. |
| REQ-EPR-005 | EPR-005 | QUAL-EPR-005 | core-runtime | M2 | 1-2 | EPR-004 | 9,17,19.2,19.7,21.005,22.1,22.2,23.B,v1.1:18 | Route each new Run through the canonical conditional transaction path with an initial solver and prevalidated slots; retain measured single-leg baseline, permitted manual pins, Auto quality/confidence objectives and frontier backstop. Derive DIRECT only from actual path; economical activation requires conservative quality and independent gate evidence. |
| REQ-EPR-006 | EPR-006 | QUAL-EPR-006 | core-runtime | M5 | 3 | EPR-017,EPR-009,M3.9 | 9.3,9.7,9.8,9.10,21.006,23.D,v1.1:18 | Execute economical initial candidate, collect static evidence, derive factual assurance and independent acceptance; activate only its prevalidated stronger-solver slot on quality rejection. Preserve original request, failed-leg evidence and remaining budget; exact-revision verified atomic apply yields a derived CASCADE path label. |
| REQ-EPR-007 | EPR-007 | QUAL-EPR-007 | core-runtime | M6 | 4 | EPR-018,M6.5 | 9.5,9.6,9.9,15,21.007,23.E,v1.1:18 | Activate a prevalidated reviewer/reviser slot only after static evidence and independent assurance/acceptance assessment requires it. Consume the EPR-018 disposable environment; pass relevant code/evidence without solver hidden reasoning; validate current-revision findings and bounded revision. CRITIQUE is a derived path label, not a template. |
| REQ-EPR-008 | EPR-008 | QUAL-EPR-008 | effects-security | M4 | 2-4 | EPR-005,M4.6 | 5,9.9,18,21.008,v1.1:18 | PolicyEnvelope defines legal models/effects, protected surfaces, minimum assurance, mandatory review/human and secret/network/deploy rules. Derive RealizedRisk from factual candidate changes; only strengthen policy minima. A generic learned risk score is not required and passing tests cannot lower assurance. |
| REQ-EPR-009 | EPR-009 | QUAL-EPR-009 | core-runtime | M4 | 2-4 | EPR-005,M4.6 | 7.9,7.10,16,21.009,23.F,v1.1:18 | Persist active conditional plan/cache/profile/epoch; reevaluate at task/compaction/demand/provider/quality/mode boundaries and switch only to confidence-feasible alternatives that remain better after lost cache, refill, cache-write, switch latency and hysteresis. Inside a transaction activate only admitted slots; different topology needs a reconciled new transaction with remaining budget. |
| REQ-EPR-010 | EPR-010 | QUAL-EPR-010 | observability | M9 | 5 | EPR-007,EPR-009 | 10,11,12.3,14,21.010,v1.1:18 | Account all inference, reviewer tool/process, verification, revision/escalation/retry/fallback attempts and cache costs. Log exact profile/policy/registry/Skill/statistics/compiler/gate/risk versions, candidate quality mean/LCB and executed slot path. Attribute request success, failed initial leg, reviewer findings/revision and acceptance/risk errors separately; preserve raw corrections/reverts/interventions under privacy policy. |
| REQ-EPR-011 | EPR-011 | QUAL-EPR-011 | eval-bench | M9 | 5 | EPR-007,EPR-010,M9.3 | 12,13.1,21.011 | Replay alternative validated plans offline on immutable sanitized snapshots and scratch worktrees with no production/tool credentials or external side effects; only reuse matching deterministic evidence; distinguish observed alternative runs from estimates. |
| REQ-EPR-012 | EPR-012 | QUAL-EPR-012 | eval-bench | M10 | 5 | EPR-011,EPR-009,EPR-019,M10.1 | 12.2,12.3,13,14,21.012,23.G,v1.1:18 | Search conditional initial solver/effort/Skills/evidence/acceptance/risk/reviewer-tool/revision/continuation parameters offline, never arbitrary graphs. Reach confidence-adjusted feasibility and independently safe gate calibration before optimizing complete cost/latency/intervention/recovery; pin holdout/propensity and controlled rollout evidence with compatible immutable policy/statistics rollback. |
| REQ-EPR-013 | EPR-013 | QUAL-EPR-013 | skills | M10 | 6 | EPR-012,M5.7 | 7.3,13.2,19.3,21.013,v1.1:18 | Use evaluation-qualified model × Skill × effort × context/harness combinations in versioned Outcome Statistics; pin content/registry/statistics versions in plan provenance. Intrinsic RequestProfile remains pure and Skills never grant reviewer canonical/persistent effects or self-promotion. |
| REQ-EPR-014 | EPR-014 | QUAL-EPR-014 | core-runtime | M2 | 1 | EPR-001 | v1.1:3,v1.1:12,v1.1:18.1,v1.1:18.7 | Implement the conditional transaction schema migration, stable slot IDs/predecessors/triggers/activation counts, pre-dispatch validation/reservation, Core admission and fenced restart in place. Legacy template labels cannot authorize slots; no runtime branch synthesis. This task supplies the shared plan validator/admission interface consumed by EPR-004/005, not a second compiler or executor. |
| REQ-EPR-015 | EPR-015 | QUAL-EPR-015 | eval-bench | M2 | 1 | EPR-002 | v1.1:6,v1.1:7,v1.1:14,v1.1:15 | Materialize immutable stats_version snapshots in existing event/artifact stores, separate from Model Registry. Keep source versions/digests, means/intervals/sample counts and exact solver/Skill/harness, joint escalation/gate/repository/verification, reviewer-family and revision-finding keys; no second memory/state owner. |
| REQ-EPR-016 | EPR-016 | QUAL-EPR-016 | model-gateway | M2 | 1 | EPR-003,EPR-015 | v1.1:4,v1.1:5,v1.1:13 | Implement reusable quality LCB/posterior feasibility for whole conditional plans, mode tau/delta and cheapest-feasible selection. Benchmark means are low-confidence priors; none-feasible selects best hard-eligible plan with QUALITY_FLOOR_INFEASIBLE, never violates budget/policy or claims target met. |
| REQ-EPR-017 | EPR-017 | QUAL-EPR-017 | verification | M4 | 2-3 | EPR-008,M4.6 | v1.1:8,v1.1:9,v1.1:11 | Consume policy-owned RealizedRisk and independently materialize AcceptanceGateResult required_assurance/evidence/verdict/missing_evidence. Static checks precede review. ACCEPT/REJECT/INCONCLUSIVE activate only precompiled valid slots or safe stop; correct tests never erase mandatory review/human requirements. |
| REQ-EPR-018 | EPR-018 | QUAL-EPR-018 | effects-security | M6 | 4 | EPR-006,M6.4 | v1.1:10,v1.1:11,v1.1:12 | Extend existing local worktree/execution/capability owners with disposable review environment: canonical tree read-only, bounded sandbox processes and ephemeral scratch writes permitted, deny-default network/secrets, no canonical mutation/Git commit/push/deploy/persistent/external actions. Default context excludes solver hidden reasoning; cleanup revokes/kills/disposes resources. |
| REQ-EPR-019 | EPR-019 | QUAL-EPR-019 | eval-bench | M9 | 5 | EPR-007,EPR-017,EPR-010 | v1.1:15,v1.1:16,v1.1:17 | Benchmark Acceptance Gate and RealizedRisk independently on representative held-out candidates with oracle labels: acceptance false accept/reject, risk false negative/positive and critical_surface_miss_rate. Pin thresholds/statistical method/samples and block promotion for unsafe gates regardless of routing savings. |

## Shared depth, ownership and ordering

At intake use doc 86 and trace production caller → canonical owner → policy → persistence → real effector → evidence/projection before editing. Adoption audit is DOCUMENTED-ONLY; future agents must repeat it against then-current code. Every task includes domain/API, storage, capabilities, real wiring, cancellation/timeouts/retries, recovery, evidence and meaningful projections. Schemas or unit tests alone cannot close product work.

EPR-000/001 provide baseline/raw direct observations and persistence. EPR-015 supplies versioned conservative statistics snapshots early; richer compound observations arrive through EPR-010 later, avoiding a learning-loop prerequisite cycle. EPR-014 supplies the shared conditional schema/validator/admission contract before the full compiler EPR-004 and runtime integration EPR-005. EPR-016 is its reusable confidence component, not a competing router. M2 uses bounded repository summaries and does not depend on M3 full indexing.

EPR-008 owns factual assurance policy, EPR-017 owns independent acceptance integration. EPR-018 owns the actual disposable review security environment, while EPR-007 uses that environment for model findings/revision; neither is a second scheduler. Local sandbox admission must be proven by EPR-018 using existing execution interfaces; cloud M8 is not an implicit prerequisite or permission to fake a sandbox. Unsupported platforms cannot admit review slots.

EPR-019 qualifies independent gate/risk safety after actual review and attribution paths exist. Full product release M10.3 depends on EPR-013 and EPR-019. Experimental disabled continuations can be tested before promotion. Static initial-only baseline migration does not activate cheap routing; any new economical or continuation policy must pass the applicable v1.1 gates in doc 61, including gate calibration. This may delay production activation beyond its nominal patch phase; quality/safety evidence takes precedence over phase labels.

All continuation shapes/bindings/limits are validated and reserved before initial execution. New required topology ends/reconciles the current transaction before separate admission with remaining request budget. No task may use a runtime recompile to invent branches, count successful escalation as initial-leg success, or grant reviewer persistent/canonical effects.

<a id="epr-000"></a>

## EPR-000 — Preserve and measure the direct baseline

- **Requirement:** REQ-EPR-000; **related preserved requirements:** REQ-EV-0029,REQ-EV-0030,REQ-EV-0112.
- **Owner / milestone / phase:** model-gateway / M2 / 0; **prerequisites:** M2.9.
- **Scope and acceptance:** Keep the existing single-model engineering path working; instrument every invocation, cache unit, retry, verification and intervention; publish fixed-revision verified-outcome baseline before routing changes.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-000 / EPR-E2E-000 — Use a real provider through Core on a real Git repository with edit/build/test/review and cancellation; retain request/event/usage IDs, build and environment digests plus cost/latency/success baseline.
- **Failure and negative proof:** EPR-FI-000 — Drop a provider stream and cancel an active attempt; incomplete usage stays unknown/estimated and no effect is repeated.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-001"></a>

## EPR-001 — Version routing contracts and durable Run state

- **Requirement:** REQ-EPR-001; **related preserved requirements:** REQ-EV-0010,REQ-EV-0112.
- **Owner / milestone / phase:** domain-events / M2 / 1; **prerequisites:** EPR-000.
- **Scope and acceptance:** Version PolicyEnvelope, RequestProfile, ModelProfile, ConditionalExecutionPlan schema 2, RoutingDecisionRecord/Trace, RealizedRisk, AcceptanceGateResult, ReviewerResult, OutcomeRecord and OutcomeStatistics refs; persist Run/slot/leg/attempt/epoch/budgets in existing stores; preserve explicit legacy decode provenance.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-001 / EPR-E2E-001 — Round-trip generated Rust/TS schemas and migrate an actual pre-plan SQLite fixture; kill Core between plan append/projection update and recover identical state; verify redacted API projections.
- **Failure and negative proof:** EPR-FI-001 — Reject missing versions, invalid money/probabilities, foreign-tenant references and stale generations; crash during migration must leave a recoverable database.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-002"></a>

## EPR-002 — Extend the Model Registry with current role bindings

- **Requirement:** REQ-EPR-002; **related preserved requirements:** REQ-EV-0112,REQ-EV-0189.
- **Owner / milestone / phase:** model-gateway / M2 / 1; **prerequisites:** EPR-001.
- **Scope and acceptance:** Keep model/provider/family/role, modality/context/tool compatibility, current economics/latency/health and governance in signed versioned Model Registry; remove empirical solver/escalation/reviewer/revision statistics to the separate versioned dataset interface; no desktop release for configuration updates.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-002 / EPR-E2E-002 — Activate signed configuration through the real Gateway; assert registry has no empirical workflow outcome fields, current capabilities/data rules filter bindings and configuration ingestion rejects embedded empirical fields; keep an external stats_version reference without producing estimates (materialization is EPR-015); retain live provider and generation proof.
- **Failure and negative proof:** EPR-FI-002 — Tamper signature, expire freshness, remove floor or reviewer and revoke a model mid-Run; prevalidated eligible fallback or explicit failure, no client secret exposure.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-003"></a>

## EPR-003 — Bootstrap and calibrate the Request Profiler

- **Requirement:** REQ-EPR-003; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / phase:** model-gateway / M2 / 1; **prerequisites:** EPR-002.
- **Scope and acceptance:** Bound feature extraction; produce catalog-independent task/domain/modifier demand, p_floor_success, confidence and OOD with versioned calibration cohort. No general LLM hot-path call; deterministic conservative fallback; initially shadow only.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-003 / EPR-E2E-003 — Benchmark fixed real repository/request corpus and untouched holdout; publish Brier/ECE, slice and OOD metrics plus extraction-inclusive p50/p95 on named hardware; price/provider/cache/allowlist changes do not change intrinsic features.
- **Failure and negative proof:** EPR-FI-003 — Disable profiler and supply stale/missing repository metadata; use a conservative eligible initial-leg conditional plan or fail; contaminated features and poor high-risk calibration block promotion.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-004"></a>

## EPR-004 — Compile and validate one bounded conditional plan

- **Requirement:** REQ-EPR-004; **related preserved requirements:** REQ-EV-0029,REQ-EV-0030.
- **Owner / milestone / phase:** model-gateway / M2 / 1; **prerequisites:** EPR-014,EPR-016.
- **Scope and acceptance:** Enumerate promoted conditional parameters and prevalidate every initial/continuation binding, trigger, isolation, assurance and worst-case total budget before dispatch. Use confidence-feasibility from EPR-016, then lowest expected complete cost; no primary DIRECT/CASCADE/CRITIQUE template dispatch or runtime branch synthesis; manual pins retain policy.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-004 / EPR-E2E-004 — Drive compiler via production Core command using actual signed registry and policy snapshots; identical inputs produce identical plans/digests; verify requested/resolved configuration and every exclusion reason with golden corpus.
- **Failure and negative proof:** EPR-FI-004 — Inject cyclic or undeclared slots, integer overflow, unavailable assurance, disallowed residency, reviewer canonical-write access, stale statistics or insufficient cap; no dispatch. Runtime must reject a gate trying to synthesize a new topology.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-005"></a>

## EPR-005 — Integrate initial execution and preserve direct baseline

- **Requirement:** REQ-EPR-005; **related preserved requirements:** REQ-EV-0029,REQ-EV-0112,REQ-EV-0256.
- **Owner / milestone / phase:** core-runtime / M2 / 1-2; **prerequisites:** EPR-004.
- **Scope and acceptance:** Route each new Run through the canonical conditional transaction path with an initial solver and prevalidated slots; retain measured single-leg baseline, permitted manual pins, Auto quality/confidence objectives and frontier backstop. Derive DIRECT only from actual path; economical activation requires conservative quality and independent gate evidence.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-005 / EPR-E2E-005 — Repeat baseline through new conditional initial-leg path using real provider/edit/test/review and restart; compare verified success, cost, latency and reliability. Prove manual pin behavior and static policy canary/rollback; classify actual DIRECT path and do not promote cheap routes without LCB and gate evidence.
- **Failure and negative proof:** EPR-FI-005 — Interrupt after typed tool dispatch and restart actual Core; reconcile unknown effect before continuing; user pin conflict cannot silently switch model or expand budget.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-006"></a>

## EPR-006 — Activate prevalidated escalation continuations

- **Requirement:** REQ-EPR-006; **related preserved requirements:** REQ-EV-0014,REQ-EV-0029.
- **Owner / milestone / phase:** core-runtime / M5 / 3; **prerequisites:** EPR-017,EPR-009,M3.9.
- **Scope and acceptance:** Execute economical initial candidate, collect static evidence, derive factual assurance and independent acceptance; activate only its prevalidated stronger-solver slot on quality rejection. Preserve original request, failed-leg evidence and remaining budget; exact-revision verified atomic apply yields a derived CASCADE path label.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-006 / EPR-E2E-006 — Live floor/provider escalation on real repository defects and accepted edits through Core; holdout measures gate false accept/reject, quality floor, total expected cost including failed floor and acceptable latency on a named slice.
- **Failure and negative proof:** EPR-FI-006 — Kill between floor result/gate/escalation; remove mandatory gate, exhaust budget or race user edits at apply; no floor auto-accept, duplicate effect or partial accepted patch.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-007"></a>

## EPR-007 — Integrate isolated review and bounded revision

- **Requirement:** REQ-EPR-007; **related preserved requirements:** REQ-EV-0219,REQ-EV-0256.
- **Owner / milestone / phase:** core-runtime / M6 / 4; **prerequisites:** EPR-018,M6.5.
- **Scope and acceptance:** Activate a prevalidated reviewer/reviser slot only after static evidence and independent assurance/acceptance assessment requires it. Consume the EPR-018 disposable environment; pass relevant code/evidence without solver hidden reasoning; validate current-revision findings and bounded revision. CRITIQUE is a derived path label, not a template.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-007 / EPR-E2E-007 — Use real provider initial solver/reviewer/reviser with candidate static evidence and EPR-018 environment; measure grounded defect/critical/API/security recall, false positives, churn and inference/tool costs; assert hidden reasoning excluded and review scratch never becomes canonical patch.
- **Failure and negative proof:** EPR-FI-007 — Attempt canonical writes, commit/push, secret/egress access, forged refs and stale findings; deny them while legitimate bounded scratch evidence execution succeeds. Missing required reviewer or exhausted revision cannot ACCEPT.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-008"></a>

## EPR-008 — Derive factual policy-owned assurance requirements

- **Requirement:** REQ-EPR-008; **related preserved requirements:** REQ-EV-0014,REQ-EV-0029.
- **Owner / milestone / phase:** effects-security / M4 / 2-4; **prerequisites:** EPR-005,M4.6.
- **Scope and acceptance:** PolicyEnvelope defines legal models/effects, protected surfaces, minimum assurance, mandatory review/human and secret/network/deploy rules. Derive RealizedRisk from factual candidate changes; only strengthen policy minima. A generic learned risk score is not required and passing tests cannot lower assurance.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-008 / EPR-E2E-008 — Change real protected/auth/migration fixtures through production paths; assert factual required assurance remains strict despite passing tests or high model confidence, and missing new continuation produces safe stop rather than synthesized branch.
- **Failure and negative proof:** EPR-FI-008 — Mutate the risk/path control and ensure tests fail; forge low learned risk or reviewer confidence, change policy during leg and deny unknown-effect retries; no weakened minimum.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-009"></a>

## EPR-009 — Persist routing epochs and switch economics

- **Requirement:** REQ-EPR-009; **related preserved requirements:** REQ-EV-0012,REQ-EV-0256.
- **Owner / milestone / phase:** core-runtime / M4 / 2-4; **prerequisites:** EPR-005,M4.6.
- **Scope and acceptance:** Persist active conditional plan/cache/profile/epoch; reevaluate at task/compaction/demand/provider/quality/mode boundaries and switch only to confidence-feasible alternatives that remain better after lost cache, refill, cache-write, switch latency and hysteresis. Inside a transaction activate only admitted slots; different topology needs a reconciled new transaction with remaining budget.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-009 / EPR-E2E-009 — Run live multi-turn provider sessions with cache metadata plus actual compaction/restart; compare stay/switch total economics and quality drift; record route and compaction epochs and switch reason.
- **Failure and negative proof:** EPR-FI-009 — Late old-model response after epoch change, cache expiry and process kill cannot overwrite newer state or reset spent/reserved budget; model identity change preserves Run/Agent identity.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-010"></a>

## EPR-010 — Capture request, leg and gate accounting/outcomes

- **Requirement:** REQ-EPR-010; **related preserved requirements:** REQ-EV-0112.
- **Owner / milestone / phase:** observability / M9 / 5; **prerequisites:** EPR-007,EPR-009.
- **Scope and acceptance:** Account all inference, reviewer tool/process, verification, revision/escalation/retry/fallback attempts and cache costs. Log exact profile/policy/registry/Skill/statistics/compiler/gate/risk versions, candidate quality mean/LCB and executed slot path. Attribute request success, failed initial leg, reviewer findings/revision and acceptance/risk errors separately; preserve raw corrections/reverts/interventions under privacy policy.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-010 / EPR-E2E-010 — Reconcile actual provider/process usage and retained events for an initial-leg failure followed by successful escalation; request success remains true but initial success false, gate corrections are independent, all versions/mean/LCB and executed slots are reconstructable; missing signals remain explicit.
- **Failure and negative proof:** EPR-FI-010 — Interrupt accounting, replay duplicate usage, deliver late invoice and apply retention deletion; no double charge, lost reservation, fabricated savings or cross-tenant telemetry.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-011"></a>

## EPR-011 — Build isolated counterfactual replay

- **Requirement:** REQ-EPR-011; **related preserved requirements:** REQ-EV-0014.
- **Owner / milestone / phase:** eval-bench / M9 / 5; **prerequisites:** EPR-007,EPR-010,M9.3.
- **Scope and acceptance:** Replay alternative validated plans offline on immutable sanitized snapshots and scratch worktrees with no production/tool credentials or external side effects; only reuse matching deterministic evidence; distinguish observed alternative runs from estimates.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-011 / EPR-E2E-011 — Execute an alternative through the real eval/Gateway path on an exact snapshot; monitor filesystem and denied egress targets to prove only scratch changes and no production effects; retain version/digest/comparability metadata.
- **Failure and negative proof:** EPR-FI-011 — Attempt replay with credentials, external target, stale evidence or mismatched revision; reject/isolate and label missing observations; property/mutation check detects removed isolation.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-012"></a>

## EPR-012 — Jointly evaluate conditional parameters and promote policy

- **Requirement:** REQ-EPR-012; **related preserved requirements:** REQ-EV-0029,REQ-EV-0030.
- **Owner / milestone / phase:** eval-bench / M10 / 5; **prerequisites:** EPR-011,EPR-009,EPR-019,M10.1.
- **Scope and acceptance:** Search conditional initial solver/effort/Skills/evidence/acceptance/risk/reviewer-tool/revision/continuation parameters offline, never arbitrary graphs. Reach confidence-adjusted feasibility and independently safe gate calibration before optimizing complete cost/latency/intervention/recovery; pin holdout/propensity and controlled rollout evidence with compatible immutable policy/statistics rollback.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-012 / EPR-E2E-012 — Use real isolated replay and controlled policy-allowed canary endpoint; prove insufficient/quality-regressing evidence blocks activation; activate and roll back signed versions under live Core with compatible registry and preserved Run state.
- **Failure and negative proof:** EPR-FI-012 — Inject unsafe exploration, mismatched registry, missing propensity/evidence, concurrent activation or active-model revocation; fail closed and prove rollback does not resurrect forbidden bindings.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-013"></a>

## EPR-013 — Qualify model × Skill outcome statistics

- **Requirement:** REQ-EPR-013; **related preserved requirements:** REQ-EV-0029,REQ-EV-0219.
- **Owner / milestone / phase:** skills / M10 / 6; **prerequisites:** EPR-012,M5.7.
- **Scope and acceptance:** Use evaluation-qualified model × Skill × effort × context/harness combinations in versioned Outcome Statistics; pin content/registry/statistics versions in plan provenance. Intrinsic RequestProfile remains pure and Skills never grant reviewer canonical/persistent effects or self-promotion.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-013 / EPR-E2E-013 — Evaluate fixed-version combinations using real skill compiler, provider, repository tests and untouched holdout; verify selected skill provenance and measured slice benefit; unqualified/revoked packs excluded without changing permission.
- **Failure and negative proof:** EPR-FI-013 — A changed/malicious Skill requesting reviewer canonical writes, credentials or self-promotion is denied; permitted scratch evidence writes remain within EPR-018 ceilings. Reject stale statistics after Skill/harness drift.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-014"></a>

## EPR-014 — Conditional plan migration and slot admission

- **Requirement:** REQ-EPR-014; **related preserved requirements:** REQ-EV-0010,REQ-EV-0012.
- **Owner / milestone / phase:** core-runtime / M2 / 1; **prerequisites:** EPR-001.
- **Scope and acceptance:** Implement the conditional transaction schema migration, stable slot IDs/predecessors/triggers/activation counts, pre-dispatch validation/reservation, Core admission and fenced restart in place. Legacy template labels cannot authorize slots; no runtime branch synthesis. This task supplies the shared plan validator/admission interface consumed by EPR-004/005, not a second compiler or executor.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-014 / EPR-E2E-014 — Migrate real pre-plan/v1.0 SQLite fixtures and send versioned plans through authenticated Core admission; validate all slots before initial dispatch, persist slot table/reservation, kill/restart during activation and recover one exact activation with unchanged budget.
- **Failure and negative proof:** EPR-FI-014 — Reject unmappable legacy template, unlisted runtime continuation, cyclic slots, stale epoch and duplicate activation; no dispatch or partial accepted patch.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-015"></a>

## EPR-015 — Versioned Outcome Statistics materialization

- **Requirement:** REQ-EPR-015; **related preserved requirements:** REQ-EV-0029,REQ-EV-0112.
- **Owner / milestone / phase:** eval-bench / M2 / 1; **prerequisites:** EPR-002.
- **Scope and acceptance:** Materialize immutable stats_version snapshots in existing event/artifact stores, separate from Model Registry. Keep source versions/digests, means/intervals/sample counts and exact solver/Skill/harness, joint escalation/gate/repository/verification, reviewer-family and revision-finding keys; no second memory/state owner.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-015 / EPR-E2E-015 — Persist and reload a real statistics snapshot sourced from attributable baseline outcomes; compiler read interface joins pinned registry and stats versions independently; repeated events do not duplicate samples and sparse priors remain low-confidence.
- **Failure and negative proof:** EPR-FI-015 — Reject wrong tenant, stale/incompatible stats, missing gate/verification key and fabricated sample count; changed registry cannot silently rewrite observations or qualify cheap plans.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-016"></a>

## EPR-016 — Confidence-adjusted feasibility and cold start

- **Requirement:** REQ-EPR-016; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / phase:** model-gateway / M2 / 1; **prerequisites:** EPR-003,EPR-015.
- **Scope and acceptance:** Implement reusable quality LCB/posterior feasibility for whole conditional plans, mode tau/delta and cheapest-feasible selection. Benchmark means are low-confidence priors; none-feasible selects best hard-eligible plan with QUALITY_FLOOR_INFEASIBLE, never violates budget/policy or claims target met.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-016 / EPR-E2E-016 — Replay representative repository/request evidence through the real compiler quality component with pinned stats/threshold versions; high mean plus weak bound fails cheap eligibility, minimum-cost feasible wins, empty feasible set emits exact infeasibility and hard-empty set fails.
- **Failure and negative proof:** EPR-FI-016 — Mutate mean-only selection, confidence method, sample threshold, missing data and switch-cost/hysteresis; tests must expose unsafe cheap eligibility and no unbounded fallback.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-017"></a>

## EPR-017 — Separate assurance classification and acceptance

- **Requirement:** REQ-EPR-017; **related preserved requirements:** REQ-EV-0014,REQ-EV-0029.
- **Owner / milestone / phase:** verification / M4 / 2-3; **prerequisites:** EPR-008,M4.6.
- **Scope and acceptance:** Consume policy-owned RealizedRisk and independently materialize AcceptanceGateResult required_assurance/evidence/verdict/missing_evidence. Static checks precede review. ACCEPT/REJECT/INCONCLUSIVE activate only precompiled valid slots or safe stop; correct tests never erase mandatory review/human requirements.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-017 / EPR-E2E-017 — Run real repository build/type/test/security/API evidence through verification route; high assurance with missing review stays INCONCLUSIVE, deterministic failure rejects, complete current evidence accepts; record independent risk and gate versions/refs across restart.
- **Failure and negative proof:** EPR-FI-017 — Forge PASS evidence on critical surface, omit human proof, stale revision or missing slot; no ACCEPT or branch generation. Mutate risk/acceptance conflation and prove independent tests fail.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-018"></a>

## EPR-018 — Isolated Non-Committing Reviewer environment

- **Requirement:** REQ-EPR-018; **related preserved requirements:** REQ-EV-0014,REQ-EV-0219.
- **Owner / milestone / phase:** effects-security / M6 / 4; **prerequisites:** EPR-006,M6.4.
- **Scope and acceptance:** Extend existing local worktree/execution/capability owners with disposable review environment: canonical tree read-only, bounded sandbox processes and ephemeral scratch writes permitted, deny-default network/secrets, no canonical mutation/Git commit/push/deploy/persistent/external actions. Default context excludes solver hidden reasoning; cleanup revokes/kills/disposes resources.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-018 / EPR-E2E-018 — Execute actual bounded test process that writes ephemeral build output in throwaway worktree; hash canonical tree unchanged, test no ambient credentials/egress and hidden-reasoning exclusion, retain trusted evidence export, kill/cancel/restart and prove scratch/process/handle cleanup.
- **Failure and negative proof:** EPR-FI-018 — Attempt symlink/mount/path escape, commit/push, canonical patch apply, secret read, network/deploy and persistent child process; deny real effects without blocking permitted scratch writes. Unsupported sandbox rejects admission.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="epr-019"></a>

## EPR-019 — Independent gate calibration release suite

- **Requirement:** REQ-EPR-019; **related preserved requirements:** REQ-EV-0029,REQ-EV-0112.
- **Owner / milestone / phase:** eval-bench / M9 / 5; **prerequisites:** EPR-007,EPR-017,EPR-010.
- **Scope and acceptance:** Benchmark Acceptance Gate and RealizedRisk independently on representative held-out candidates with oracle labels: acceptance false accept/reject, risk false negative/positive and critical_surface_miss_rate. Pin thresholds/statistical method/samples and block promotion for unsafe gates regardless of routing savings.
- **Production wiring:** follow the existing owners and algorithms in docs 27/38; audit the first missing link before modifying source. Do not build a duplicate compiler, executor, storage authority or policy service.
- **Real qualification:** QUAL-EPR-019 / EPR-E2E-019 — Run real verifier/assurance paths on separate held-out candidate/critical-surface corpora; persist oracle/candidate/gate/risk versions and independent rates/intervals; improved router cost with unsafe gate or critical miss must fail release check.
- **Failure and negative proof:** EPR-FI-019 — Inject mislabeled/contaminated holdout, suppress risk misses or misattribute successful escalation to first leg; independent metric and rollout mutation checks must fail closed.
- **Evidence:** exact build/repository/environment/provider/config/policy/registry/Skill/statistics/compiler/gate/risk versions; event/slot/leg/attempt/effect refs; cost, qualification and fault artifacts under existing evidence manifest rules.
- **Completion:** production-equivalent real proof, applicable gate evidence and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

