# EPR v1.1 supersession decision

**Decision Record:** DR-EPR-2026-09-05-v1.1  
**Status:** APPROVED for dossier adoption; implementation and activation remain evidence-gated.  
**Approval:** On 2026-09-05 the user explicitly requested integration of the new v1.1 patch, updates to the dossier/graphs/tasks, and resealing. This authorizes amendments to the conflicting locked EPR clauses and additive requirements below.  
**Source:** [MODBIT-PATCH-EPR-2026-09-05-v1.1](../MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md). Both root source patches remain immutable; their SHA-256 values are retained in the graph and manifest.

## Trigger, evidence and existing behavior

The V3.2 dossier adopted v1.0 as distinct bounded workflow templates, mean-quality utility scoring, registry-embedded empirical profiles and an absolute reviewer tool-write prohibition. The new patch explicitly supersedes those clauses. The workspace still contains dossier/tooling only and no Git repository or product build. Product work is NOT_STARTED. Initial integrity differs from the prior seal only because the newly supplied patch is not yet listed. Baseline digests: `../evidence/dossier-epr-v1.1/baseline.json`.

## Approved replacements and surviving authority

| Superseded active clause | v1.1 authority | Implementation consequence |
|---|---|---|
| DIRECT/CASCADE/CRITIQUE as primary templates; ADR-R-039/042 | ADR-R-049/054 | One ConditionalExecutionPlan, one initial leg, prevalidated finite continuation slots; names are derived executed-path labels only |
| Soft expected-quality-minus-cost utility; ADR-R-041/047 scoring clauses | ADR-R-050 | Minimize expected cost among confidence-adjusted quality-feasible plans; quality/confidence floors are mode settings; explicit QUALITY_FLOOR_INFEASIBLE if none qualifies |
| Solver/escalation/reviewer/revision empirical profiles inside Model Registry | ADR-R-051 | Separately versioned Outcome Statistics data joins the compiler; registry keeps binding/capability/economics/health/governance |
| Risk classification mixed with evidence acceptance; ADR-R-046 interpretation | ADR-R-052 | Policy/factual RealizedRisk sets minimum assurance; AcceptanceGateResult independently checks evidence |
| Effect-less critic forbids even scratch writes/execution; ADR-R-043 | ADR-R-053 | Isolated Non-Committing Reviewer: canonical tracked tree read-only; bounded sandboxed processes and ephemeral writes only in disposable review environment |
| Safe-boundary runtime recompilation adds branches | ADR-R-054 | Only previously validated slots may activate inside a transaction; a new topology requires ending/reconciling it before compiling a distinct transaction |
| Request success treated as evidence of earlier solver success | ADR-R-055 | Separate request, leg and gate attribution; successful escalation cannot credit a failed first leg |
| Aggregate routing gain can obscure gate/risk regression | ADR-R-056 | Independent acceptance/risk calibration, critical-surface miss and false-accept limits block release regardless of router gains |

Existing policy/effect approval, credentials, data residency, tenant boundaries, Core/Run/Session, worktrees/checkpoints, context, skills, cancellation, retries, budget inheritance, atomic workspace apply, real verification and offline controlled promotion remain mandatory. Independent review does not receive solver hidden reasoning. Reviewer network/secrets deny by default; canonical mutation, Git commit/push, deployment and persistent/external actions remain forbidden. Trusted Core may export bounded evidence artifacts through the existing evidence owner; that is not reviewer authority to persist changes.

## Ownership and task collision audit

No ADR-R-049..056 or EPR-014..019 IDs were allocated in the pre-change graph. They retain the patch's semantic order. Existing EPR-000..013 are amended in place to v1.1 so development never builds obsolete v1.0 templates first.

| Delta task | Distinct work and existing implementation to extend |
|---|---|
| EPR-014 | Conditional transaction admission/migration, continuation selection/fencing and restart; extends Core RunSteps and EPR-001/004/005, not another executor |
| EPR-015 | Versioned Outcome Statistics materialization and compiler read interface in existing Eval Harness/event/artifact storage; distinct from model configuration and raw outcome capture |
| EPR-016 | Conservative confidence-bound/posterior feasibility, cold-start and infeasible-floor classification consumed by the existing compiler |
| EPR-017 | Independent acceptance contract and assurance-evidence integration; consumes policy-owned factual risk from EPR-008 |
| EPR-018 | Real disposable review environment, process/path/network/secret isolation and cleanup; consumed by EPR-007's model/revision flow |
| EPR-019 | Independent acceptance and realized-risk calibration suites with release-critical thresholds; distinct from profiler and routing benefit benchmarks |

These are delta slices with one writer/owner each, not duplicate implementations of their parent tasks. Add REQ-EPR/QUAL-EPR-014..019 and real/fault scenarios, preserving existing IDs, owners and all 291 REQ-EV rows. There are 20 EPR task/requirement/qualification triplets after this amendment. No new canonical subsystem is created: Outcome Statistics is versioned derived data under eval-bench using existing stores, not a second event, protocol or memory authority.

## Migration and compatibility

New Runs persist ConditionalExecutionPlan schema 2 with compiled slot IDs, bound triggers, model/config versions, termination/retry bounds and entire-transaction budget before first dispatch. v1.0 ExecutionPlan, QualityGateResult and CriticResult remain explicit legacy decode adapters to conditional plan, AcceptanceGateResult and ReviewerResult; new writers emit canonical v1.1 types. Only lossless, validated mappings are executable. A legacy template name cannot authorize a missing continuation, reviewer capability, confidence bound or statistics version. Missing migration evidence yields explicit legacy/infeasible/unsupported state; do not invent historical LCBs or samples.

Unknown major/workflow semantics block mutation. Reconcile in-flight effects and usage before ending a transaction or migrating its continuation state. New transactions inherit remaining request/daily reservations, never a fresh budget. No policy rollout silently edits a running slot graph. Phase-2 static-initial-leg qualification retains the earlier controlled rollout sequence but now also requires confidence feasibility, independent acceptance/risk calibration and slot validation. Full rollout additionally requires reviewer isolation and all applicable gate evidence in doc 61.

## Test impact, rollback and release conditions

Update docs 27/38/49/61 and their current subsystem consumers. Test actual conditional slot activation/restart, sparse-data confidence rejection and QUALITY_FLOOR_INFEASIBLE, statistics/version mismatch, independent risk/acceptance outcomes, allowed ephemeral reviewer writes versus denied canonical/external changes, hidden-reasoning exclusion, request/leg/gate attribution and gate safety regression. Schema/Markdown existence is not a product proof.

Rollback uses a previous-good immutable policy only if compatible with current plan/schema, registry/statistics, assurance and reviewer constraints. It cannot reactivate superseded templates, soften a quality floor, restore forbidden reviewer effects or introduce new runtime branches. Finish/reconcile the current transaction before any incompatible policy change. Rollback remains possible without a desktop release. Reversing this dossier amendment requires another explicit approved Decision Record.

## Handoff

DOC-EPR-002 records this dossier-only work and its stage applicability, exact revision, tests and reseal evidence in `95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md`. The earlier doc 05/94 and evidence bundle describe the historical v1.0 adoption. Their non-conflicting decisions survive; superseded clauses do not govern new development. The current numbered docs are authoritative over either root patch's unexecuted adoption checklist.
