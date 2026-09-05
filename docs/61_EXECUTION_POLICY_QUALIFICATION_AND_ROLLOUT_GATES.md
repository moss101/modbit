# Execution policy qualification and rollout gates

> **Authority:** DR-EPR-2026-09-05-v1.1, ADR-R-039..056. **Product proof:** NOT_STARTED.  
> Doc 49 maps 20 requirements/tasks to these 20 QUAL-EPR and 40 real/fault scenarios. These are product tests to implement, not results of dossier checks. Active conditional contracts: docs 27/38.

## Qualification matrix

Use authenticated production command routes into actual Core, provider, policy, verifier, Git/filesystem/process/database boundaries. Retain immutable candidate/plan/slot/attempt and all registry/statistics/gate/risk versions. Mocks supplement deterministic edge cases but cannot close product capabilities.

| Qualification | Requirement | Owner | Real qualification | Failure proof |
|---|---|---|---|---|
| QUAL-EPR-000 | REQ-EPR-000 | model-gateway | Use a real provider through Core on a real Git repository with edit/build/test/review and cancellation; retain request/event/usage IDs, build and environment digests plus cost/latency/success baseline. | Drop a provider stream and cancel an active attempt; incomplete usage stays unknown/estimated and no effect is repeated. |
| QUAL-EPR-001 | REQ-EPR-001 | domain-events | Round-trip generated Rust/TS schemas and migrate an actual pre-plan SQLite fixture; kill Core between plan append/projection update and recover identical state; verify redacted API projections. | Reject missing versions, invalid money/probabilities, foreign-tenant references and stale generations; crash during migration must leave a recoverable database. |
| QUAL-EPR-002 | REQ-EPR-002 | model-gateway | Activate signed configuration through the real Gateway; assert registry has no empirical workflow outcome fields, current capabilities/data rules filter bindings and configuration ingestion rejects embedded empirical fields; keep an external stats_version reference without producing estimates (materialization is EPR-015); retain live provider and generation proof. | Tamper signature, expire freshness, remove floor or reviewer and revoke a model mid-Run; prevalidated eligible fallback or explicit failure, no client secret exposure. |
| QUAL-EPR-003 | REQ-EPR-003 | model-gateway | Benchmark fixed real repository/request corpus and untouched holdout; publish Brier/ECE, slice and OOD metrics plus extraction-inclusive p50/p95 on named hardware; price/provider/cache/allowlist changes do not change intrinsic features. | Disable profiler and supply stale/missing repository metadata; use a conservative eligible initial-leg conditional plan or fail; contaminated features and poor high-risk calibration block promotion. |
| QUAL-EPR-004 | REQ-EPR-004 | model-gateway | Drive compiler via production Core command using actual signed registry and policy snapshots; identical inputs produce identical plans/digests; verify requested/resolved configuration and every exclusion reason with golden corpus. | Inject cyclic or undeclared slots, integer overflow, unavailable assurance, disallowed residency, reviewer canonical-write access, stale statistics or insufficient cap; no dispatch. Runtime must reject a gate trying to synthesize a new topology. |
| QUAL-EPR-005 | REQ-EPR-005 | core-runtime | Repeat baseline through new conditional initial-leg path using real provider/edit/test/review and restart; compare verified success, cost, latency and reliability. Prove manual pin behavior and static policy canary/rollback; classify actual DIRECT path and do not promote cheap routes without LCB and gate evidence. | Interrupt after typed tool dispatch and restart actual Core; reconcile unknown effect before continuing; user pin conflict cannot silently switch model or expand budget. |
| QUAL-EPR-006 | REQ-EPR-006 | core-runtime | Live floor/provider escalation on real repository defects and accepted edits through Core; holdout measures gate false accept/reject, quality floor, total expected cost including failed floor and acceptable latency on a named slice. | Kill between floor result/gate/escalation; remove mandatory gate, exhaust budget or race user edits at apply; no floor auto-accept, duplicate effect or partial accepted patch. |
| QUAL-EPR-007 | REQ-EPR-007 | core-runtime | Use real provider initial solver/reviewer/reviser with candidate static evidence and EPR-018 environment; measure grounded defect/critical/API/security recall, false positives, churn and inference/tool costs; assert hidden reasoning excluded and review scratch never becomes canonical patch. | Attempt canonical writes, commit/push, secret/egress access, forged refs and stale findings; deny them while legitimate bounded scratch evidence execution succeeds. Missing required reviewer or exhausted revision cannot ACCEPT. |
| QUAL-EPR-008 | REQ-EPR-008 | effects-security | Change real protected/auth/migration fixtures through production paths; assert factual required assurance remains strict despite passing tests or high model confidence, and missing new continuation produces safe stop rather than synthesized branch. | Mutate the risk/path control and ensure tests fail; forge low learned risk or reviewer confidence, change policy during leg and deny unknown-effect retries; no weakened minimum. |
| QUAL-EPR-009 | REQ-EPR-009 | core-runtime | Run live multi-turn provider sessions with cache metadata plus actual compaction/restart; compare stay/switch total economics and quality drift; record route and compaction epochs and switch reason. | Late old-model response after epoch change, cache expiry and process kill cannot overwrite newer state or reset spent/reserved budget; model identity change preserves Run/Agent identity. |
| QUAL-EPR-010 | REQ-EPR-010 | observability | Reconcile actual provider/process usage and retained events for an initial-leg failure followed by successful escalation; request success remains true but initial success false, gate corrections are independent, all versions/mean/LCB and executed slots are reconstructable; missing signals remain explicit. | Interrupt accounting, replay duplicate usage, deliver late invoice and apply retention deletion; no double charge, lost reservation, fabricated savings or cross-tenant telemetry. |
| QUAL-EPR-011 | REQ-EPR-011 | eval-bench | Execute an alternative through the real eval/Gateway path on an exact snapshot; monitor filesystem and denied egress targets to prove only scratch changes and no production effects; retain version/digest/comparability metadata. | Attempt replay with credentials, external target, stale evidence or mismatched revision; reject/isolate and label missing observations; property/mutation check detects removed isolation. |
| QUAL-EPR-012 | REQ-EPR-012 | eval-bench | Use real isolated replay and controlled policy-allowed canary endpoint; prove insufficient/quality-regressing evidence blocks activation; activate and roll back signed versions under live Core with compatible registry and preserved Run state. | Inject unsafe exploration, mismatched registry, missing propensity/evidence, concurrent activation or active-model revocation; fail closed and prove rollback does not resurrect forbidden bindings. |
| QUAL-EPR-013 | REQ-EPR-013 | skills | Evaluate fixed-version combinations using real skill compiler, provider, repository tests and untouched holdout; verify selected skill provenance and measured slice benefit; unqualified/revoked packs excluded without changing permission. | A changed/malicious Skill requesting reviewer canonical writes, credentials or self-promotion is denied; permitted scratch evidence writes remain within EPR-018 ceilings. Reject stale statistics after Skill/harness drift. |
| QUAL-EPR-014 | REQ-EPR-014 | core-runtime | Migrate real pre-plan/v1.0 SQLite fixtures and send versioned plans through authenticated Core admission; validate all slots before initial dispatch, persist slot table/reservation, kill/restart during activation and recover one exact activation with unchanged budget. | Reject unmappable legacy template, unlisted runtime continuation, cyclic slots, stale epoch and duplicate activation; no dispatch or partial accepted patch. |
| QUAL-EPR-015 | REQ-EPR-015 | eval-bench | Persist and reload a real statistics snapshot sourced from attributable baseline outcomes; compiler read interface joins pinned registry and stats versions independently; repeated events do not duplicate samples and sparse priors remain low-confidence. | Reject wrong tenant, stale/incompatible stats, missing gate/verification key and fabricated sample count; changed registry cannot silently rewrite observations or qualify cheap plans. |
| QUAL-EPR-016 | REQ-EPR-016 | model-gateway | Replay representative repository/request evidence through the real compiler quality component with pinned stats/threshold versions; high mean plus weak bound fails cheap eligibility, minimum-cost feasible wins, empty feasible set emits exact infeasibility and hard-empty set fails. | Mutate mean-only selection, confidence method, sample threshold, missing data and switch-cost/hysteresis; tests must expose unsafe cheap eligibility and no unbounded fallback. |
| QUAL-EPR-017 | REQ-EPR-017 | verification | Run real repository build/type/test/security/API evidence through verification route; high assurance with missing review stays INCONCLUSIVE, deterministic failure rejects, complete current evidence accepts; record independent risk and gate versions/refs across restart. | Forge PASS evidence on critical surface, omit human proof, stale revision or missing slot; no ACCEPT or branch generation. Mutate risk/acceptance conflation and prove independent tests fail. |
| QUAL-EPR-018 | REQ-EPR-018 | effects-security | Execute actual bounded test process that writes ephemeral build output in throwaway worktree; hash canonical tree unchanged, test no ambient credentials/egress and hidden-reasoning exclusion, retain trusted evidence export, kill/cancel/restart and prove scratch/process/handle cleanup. | Attempt symlink/mount/path escape, commit/push, canonical patch apply, secret read, network/deploy and persistent child process; deny real effects without blocking permitted scratch writes. Unsupported sandbox rejects admission. |
| QUAL-EPR-019 | REQ-EPR-019 | eval-bench | Run real verifier/assurance paths on separate held-out candidate/critical-surface corpora; persist oracle/candidate/gate/risk versions and independent rates/intervals; improved router cost with unsafe gate or critical miss must fail release check. | Inject mislabeled/contaminated holdout, suppress risk misses or misattribute successful escalation to first leg; independent metric and rollout mutation checks must fail closed. |

## Release and promotion gates

Each gate's satisfaction is recorded on its graph node with `python3 tools/graph.py attest EPR-GATE-x --evidence ...` only after every required task is COMPLETE and the gate's own evidence below exists; M10 rolls up GATED, not COMPLETE, until all seven gates are SATISFIED (`93_STATUS_VOCABULARY_AND_LIFECYCLE.md`).

Gate nodes are evidence criteria, not task status or executable workflow selectors. Current aliases DIRECT/CASCADE/CRITIQUE name observed path slices. Confidence bounds predict plan outcomes; deterministic/current acceptance evidence still decides actual completion. Gate/risk safety is independently release-critical.

| Gate | Name | Required tasks | Acceptance |
|---|---|---|---|
| EPR-GATE-A | Conditional correctness and assurance | EPR-001,EPR-002,EPR-004,EPR-005,EPR-008,EPR-014,EPR-017 | Schema-2 conditional admission, every slot prevalidated/budgeted, finite retries/termination, no generated branch, separate risk/acceptance and no partial apply |
| EPR-GATE-B | Initial-leg DIRECT baseline non-regression | EPR-000,EPR-005 | Actual initial-to-accept path preserves verified success, cost/latency tolerances, recovery/tool reliability and complete direct-attempt accounting |
| EPR-GATE-C | Profiler and confidence-adjusted feasibility | EPR-002,EPR-003,EPR-015,EPR-016 | Intrinsic profiler calibration/OOD, versioned representative statistics, LCB/posterior tau/delta, cold-start rejection, lowest-cost feasible and explicit QUALITY_FLOOR_INFEASIBLE |
| EPR-GATE-D | Escalation CASCADE path benefit | EPR-006,EPR-017,EPR-019 | Whole-plan conservative quality floor, lower complete expected cost on approved slice, bounded rejection/escalation latency; acceptance/risk error thresholds independently pass |
| EPR-GATE-E | Isolated review CRITIQUE path benefit | EPR-007,EPR-018,EPR-019 | Relational defect/critical recall and success gain; acceptable false positives/churn/inference/tool/process cost; allowed ephemeral work and denied canonical/persistent/external effects; no solver hidden reasoning |
| EPR-GATE-F | Multi-turn economics | EPR-000,EPR-009,EPR-016 | Confidence-feasible switch remains better after lost cache/refill/write/latency/hysteresis, no unnecessary switching regression, fenced slot/restart behavior |
| EPR-GATE-G | Independently safe gate calibration and rollout | EPR-010,EPR-011,EPR-012,EPR-013,EPR-019 | Separate request/leg/gate attribution, holdout acceptance false accept/reject and realized-risk false negative/positive plus critical_surface_miss_rate within approved limits; controlled promotion/propensity, safe replay, compatible previous-good rollback |

EPR-019 is release-critical for every newly optimized policy, even if it tends to produce only DIRECT outcomes. Savings or higher routing accuracy cannot waive a gate failure. EPR-012 implements promotion using already-produced calibration evidence; EPR-013 subsequently qualifies Skill variants. G's Skill-specific requirement applies only when those variants activate, preventing an implementation dependency cycle.

## Staged baseline and activation

Initial implementation migration may preserve the previously measured initial-leg-only configuration under A/B's applicable conditional-contract checks and normal existing assurance, while the new routing capabilities remain disabled. This does not qualify a new cheap model, substitute a mean for LCB, bypass assurance or claim full gate completion.

New economical initial-solver policies require A/B/C/F, EPR-019 independent gate calibration and G's applicable static-policy holdout/shadow/canary/monitoring/rollback evidence. EPR-005 provides the existing static publisher/canary/rollback route; EPR-012 extends that same owner with joint offline parameter search. Learned/continuation promotion additionally requires EPR-010/011/012 and D for escalation slots or E for reviewer slots. Skill variants also require EPR-013. Phase labels describe intended progression, not exemptions from prerequisite proof.

## Thresholds, priors and independent gate calibration

Before measurement publish an approved immutable threshold profile: mode tau/delta and LCB/posterior method, representative sample minima, prior/confidence policy, dataset/slice/lineage splits, Brier/ECE/OOD bounds, latency/cost tolerances, reviewer recall/churn limits, and separate acceptance_false_accept_rate, acceptance_false_reject_rate, realized_risk_false_negative_rate, realized_risk_false_positive_rate, critical_surface_miss_rate. Specify sample-size/power, confidence intervals, critical-surface oracle labels, correction windows and rollback triggers. Missing thresholds fail promotion. No numeric empirical pass rate is invented by this dossier.

Published/internal benchmark priors remain low-confidence until product-representative samples accumulate. High mean/weak lower bound cannot qualify cheap plans. Fallback below quality floor must retain QUALITY_FLOOR_INFEASIBLE and never claim predicted target success; hard policy, budget, isolation and actual evidence minima still hold. Test the hard-empty set separately: it must fail, not execute a forbidden model.

Use separate acceptance-correctness and factual risk/critical-surface labeled corpora; tune/validate apart from untouched holdout, preventing task/repository lineage leakage. Evaluate false accept/reject and risk false negatives/positives independently of plan success or routing savings. A false acceptance followed by successful repair remains a gate error; a failed cheap leg followed by frontier success remains an initial-leg failure. Retain raw request/leg/gate signals with exact candidate, gate/risk and later-observation references.

Profile extraction-inclusive targets remain p50 <25 ms where practical and p95 <50 ms on declared hardware. Plan economics include every inference leg, reviewer tool/process/evidence cost, escalation/revision, switch/refill, verification, retries, human intervention and recovery. Preserve units or versioned conversions. Observed replay and estimated counterfactual comparisons remain distinct.

Randomized exploration stays among validated eligible plans with true choice probability, controlled bounds and privacy/tenant restrictions; critical/high-assurance use requires explicit policy authorization. Record every candidate's mean/LCB/cost/latency/eligibility/exclusion and immutable inputs for exact decision replay. Holdout alone cannot promote when production traffic exists: controlled shadow/canary/A-B or explicitly approved equivalent plus tested rollback is required.

## Real-system and failure scenarios

Common setup: production-equivalent Core via authenticated transport, real repository and durable stores, dedicated provider credentials only inside Gateway, actual tools/processes and revision-bound evidence. The reviewer sandbox proof must demonstrate both permitted ephemeral writes/execution and forbidden canonical/external effects; an all-tools-denied mock does not qualify it. Cleanup must kill real processes/revoke handles/dispose scratch after timeout/cancel/crash.

### EPR-E2E-000 — Preserve and measure the direct baseline

Use a real provider through Core on a real Git repository with edit/build/test/review and cancellation; retain request/event/usage IDs, build and environment digests plus cost/latency/success baseline. Assert REQ-EPR-000 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-000; do not confuse disabled experimental proof with policy activation.

### EPR-FI-000 — Failure isolation for preserve and measure the direct baseline

Drop a provider stream and cancel an active attempt; incomplete usage stays unknown/estimated and no effect is repeated. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-001 — Version routing contracts and durable Run state

Round-trip generated Rust/TS schemas and migrate an actual pre-plan SQLite fixture; kill Core between plan append/projection update and recover identical state; verify redacted API projections. Assert REQ-EPR-001 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-001; do not confuse disabled experimental proof with policy activation.

### EPR-FI-001 — Failure isolation for version routing contracts and durable run state

Reject missing versions, invalid money/probabilities, foreign-tenant references and stale generations; crash during migration must leave a recoverable database. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-002 — Extend the Model Registry with current role bindings

Activate signed configuration through the real Gateway; assert registry has no empirical workflow outcome fields, current capabilities/data rules filter bindings and configuration ingestion rejects embedded empirical fields; keep an external stats_version reference without producing estimates (materialization is EPR-015); retain live provider and generation proof. Assert REQ-EPR-002 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-002; do not confuse disabled experimental proof with policy activation.

### EPR-FI-002 — Failure isolation for extend the model registry with current role bindings

Tamper signature, expire freshness, remove floor or reviewer and revoke a model mid-Run; prevalidated eligible fallback or explicit failure, no client secret exposure. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-003 — Bootstrap and calibrate the Request Profiler

Benchmark fixed real repository/request corpus and untouched holdout; publish Brier/ECE, slice and OOD metrics plus extraction-inclusive p50/p95 on named hardware; price/provider/cache/allowlist changes do not change intrinsic features. Assert REQ-EPR-003 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-003; do not confuse disabled experimental proof with policy activation.

### EPR-FI-003 — Failure isolation for bootstrap and calibrate the request profiler

Disable profiler and supply stale/missing repository metadata; use a conservative eligible initial-leg conditional plan or fail; contaminated features and poor high-risk calibration block promotion. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-004 — Compile and validate one bounded conditional plan

Drive compiler via production Core command using actual signed registry and policy snapshots; identical inputs produce identical plans/digests; verify requested/resolved configuration and every exclusion reason with golden corpus. Assert REQ-EPR-004 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-004; do not confuse disabled experimental proof with policy activation.

### EPR-FI-004 — Failure isolation for compile and validate one bounded conditional plan

Inject cyclic or undeclared slots, integer overflow, unavailable assurance, disallowed residency, reviewer canonical-write access, stale statistics or insufficient cap; no dispatch. Runtime must reject a gate trying to synthesize a new topology. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-005 — Integrate initial execution and preserve direct baseline

Repeat baseline through new conditional initial-leg path using real provider/edit/test/review and restart; compare verified success, cost, latency and reliability. Prove manual pin behavior and static policy canary/rollback; classify actual DIRECT path and do not promote cheap routes without LCB and gate evidence. Assert REQ-EPR-005 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-005; do not confuse disabled experimental proof with policy activation.

### EPR-FI-005 — Failure isolation for integrate initial execution and preserve direct baseline

Interrupt after typed tool dispatch and restart actual Core; reconcile unknown effect before continuing; user pin conflict cannot silently switch model or expand budget. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-006 — Activate prevalidated escalation continuations

Live floor/provider escalation on real repository defects and accepted edits through Core; holdout measures gate false accept/reject, quality floor, total expected cost including failed floor and acceptable latency on a named slice. Assert REQ-EPR-006 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-006; do not confuse disabled experimental proof with policy activation.

### EPR-FI-006 — Failure isolation for activate prevalidated escalation continuations

Kill between floor result/gate/escalation; remove mandatory gate, exhaust budget or race user edits at apply; no floor auto-accept, duplicate effect or partial accepted patch. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-007 — Integrate isolated review and bounded revision

Use real provider initial solver/reviewer/reviser with candidate static evidence and EPR-018 environment; measure grounded defect/critical/API/security recall, false positives, churn and inference/tool costs; assert hidden reasoning excluded and review scratch never becomes canonical patch. Assert REQ-EPR-007 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-007; do not confuse disabled experimental proof with policy activation.

### EPR-FI-007 — Failure isolation for integrate isolated review and bounded revision

Attempt canonical writes, commit/push, secret/egress access, forged refs and stale findings; deny them while legitimate bounded scratch evidence execution succeeds. Missing required reviewer or exhausted revision cannot ACCEPT. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-008 — Derive factual policy-owned assurance requirements

Change real protected/auth/migration fixtures through production paths; assert factual required assurance remains strict despite passing tests or high model confidence, and missing new continuation produces safe stop rather than synthesized branch. Assert REQ-EPR-008 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-008; do not confuse disabled experimental proof with policy activation.

### EPR-FI-008 — Failure isolation for derive factual policy-owned assurance requirements

Mutate the risk/path control and ensure tests fail; forge low learned risk or reviewer confidence, change policy during leg and deny unknown-effect retries; no weakened minimum. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-009 — Persist routing epochs and switch economics

Run live multi-turn provider sessions with cache metadata plus actual compaction/restart; compare stay/switch total economics and quality drift; record route and compaction epochs and switch reason. Assert REQ-EPR-009 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-009; do not confuse disabled experimental proof with policy activation.

### EPR-FI-009 — Failure isolation for persist routing epochs and switch economics

Late old-model response after epoch change, cache expiry and process kill cannot overwrite newer state or reset spent/reserved budget; model identity change preserves Run/Agent identity. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-010 — Capture request, leg and gate accounting/outcomes

Reconcile actual provider/process usage and retained events for an initial-leg failure followed by successful escalation; request success remains true but initial success false, gate corrections are independent, all versions/mean/LCB and executed slots are reconstructable; missing signals remain explicit. Assert REQ-EPR-010 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-010; do not confuse disabled experimental proof with policy activation.

### EPR-FI-010 — Failure isolation for capture request, leg and gate accounting/outcomes

Interrupt accounting, replay duplicate usage, deliver late invoice and apply retention deletion; no double charge, lost reservation, fabricated savings or cross-tenant telemetry. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-011 — Build isolated counterfactual replay

Execute an alternative through the real eval/Gateway path on an exact snapshot; monitor filesystem and denied egress targets to prove only scratch changes and no production effects; retain version/digest/comparability metadata. Assert REQ-EPR-011 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-011; do not confuse disabled experimental proof with policy activation.

### EPR-FI-011 — Failure isolation for build isolated counterfactual replay

Attempt replay with credentials, external target, stale evidence or mismatched revision; reject/isolate and label missing observations; property/mutation check detects removed isolation. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-012 — Jointly evaluate conditional parameters and promote policy

Use real isolated replay and controlled policy-allowed canary endpoint; prove insufficient/quality-regressing evidence blocks activation; activate and roll back signed versions under live Core with compatible registry and preserved Run state. Assert REQ-EPR-012 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-012; do not confuse disabled experimental proof with policy activation.

### EPR-FI-012 — Failure isolation for jointly evaluate conditional parameters and promote policy

Inject unsafe exploration, mismatched registry, missing propensity/evidence, concurrent activation or active-model revocation; fail closed and prove rollback does not resurrect forbidden bindings. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-013 — Qualify model × Skill outcome statistics

Evaluate fixed-version combinations using real skill compiler, provider, repository tests and untouched holdout; verify selected skill provenance and measured slice benefit; unqualified/revoked packs excluded without changing permission. Assert REQ-EPR-013 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-013; do not confuse disabled experimental proof with policy activation.

### EPR-FI-013 — Failure isolation for qualify model × skill outcome statistics

A changed/malicious Skill requesting reviewer canonical writes, credentials or self-promotion is denied; permitted scratch evidence writes remain within EPR-018 ceilings. Reject stale statistics after Skill/harness drift. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-014 — Conditional plan migration and slot admission

Migrate real pre-plan/v1.0 SQLite fixtures and send versioned plans through authenticated Core admission; validate all slots before initial dispatch, persist slot table/reservation, kill/restart during activation and recover one exact activation with unchanged budget. Assert REQ-EPR-014 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-014; do not confuse disabled experimental proof with policy activation.

### EPR-FI-014 — Failure isolation for conditional plan migration and slot admission

Reject unmappable legacy template, unlisted runtime continuation, cyclic slots, stale epoch and duplicate activation; no dispatch or partial accepted patch. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-015 — Versioned Outcome Statistics materialization

Persist and reload a real statistics snapshot sourced from attributable baseline outcomes; compiler read interface joins pinned registry and stats versions independently; repeated events do not duplicate samples and sparse priors remain low-confidence. Assert REQ-EPR-015 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-015; do not confuse disabled experimental proof with policy activation.

### EPR-FI-015 — Failure isolation for versioned outcome statistics materialization

Reject wrong tenant, stale/incompatible stats, missing gate/verification key and fabricated sample count; changed registry cannot silently rewrite observations or qualify cheap plans. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-016 — Confidence-adjusted feasibility and cold start

Replay representative repository/request evidence through the real compiler quality component with pinned stats/threshold versions; high mean plus weak bound fails cheap eligibility, minimum-cost feasible wins, empty feasible set emits exact infeasibility and hard-empty set fails. Assert REQ-EPR-016 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-016; do not confuse disabled experimental proof with policy activation.

### EPR-FI-016 — Failure isolation for confidence-adjusted feasibility and cold start

Mutate mean-only selection, confidence method, sample threshold, missing data and switch-cost/hysteresis; tests must expose unsafe cheap eligibility and no unbounded fallback. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-017 — Separate assurance classification and acceptance

Run real repository build/type/test/security/API evidence through verification route; high assurance with missing review stays INCONCLUSIVE, deterministic failure rejects, complete current evidence accepts; record independent risk and gate versions/refs across restart. Assert REQ-EPR-017 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-017; do not confuse disabled experimental proof with policy activation.

### EPR-FI-017 — Failure isolation for separate assurance classification and acceptance

Forge PASS evidence on critical surface, omit human proof, stale revision or missing slot; no ACCEPT or branch generation. Mutate risk/acceptance conflation and prove independent tests fail. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-018 — Isolated Non-Committing Reviewer environment

Execute actual bounded test process that writes ephemeral build output in throwaway worktree; hash canonical tree unchanged, test no ambient credentials/egress and hidden-reasoning exclusion, retain trusted evidence export, kill/cancel/restart and prove scratch/process/handle cleanup. Assert REQ-EPR-018 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-018; do not confuse disabled experimental proof with policy activation.

### EPR-FI-018 — Failure isolation for isolated non-committing reviewer environment

Attempt symlink/mount/path escape, commit/push, canonical patch apply, secret read, network/deploy and persistent child process; deny real effects without blocking permitted scratch writes. Unsupported sandbox rejects admission. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

### EPR-E2E-019 — Independent gate calibration release suite

Run real verifier/assurance paths on separate held-out candidate/critical-surface corpora; persist oracle/candidate/gate/risk versions and independent rates/intervals; improved router cost with unsafe gate or critical miss must fail release check. Assert REQ-EPR-019 through production routing; preserve current-revision outcomes, exact slot activation/costs and named evidence. Link the run to QUAL-EPR-019; do not confuse disabled experimental proof with policy activation.

### EPR-FI-019 — Failure isolation for independent gate calibration release suite

Inject mislabeled/contaminated holdout, suppress risk misses or misattribute successful escalation to first leg; independent metric and rollout mutation checks must fail closed. Retain before/after canonical/scratch state, durable projections, dispatch/effect and budget evidence. For authority, risk, isolation, idempotency, version/fencing, privacy or evidence controls, break one control in a disposable test build and prove the test fails; restore and rerun the positive real-system path. No disabled security in positive proofs.

## Source example fixtures

Localized edit selects cheap initial leg only with representative conservative feasibility and current acceptance. Uncertain bug activates precompiled escalation, preserving initial failure attribution. Broad migration may select frontier directly or flag infeasible quality. Authentication work derives mandatory assurance/review/human requirements independently of passing tests. Cached multi-turn work switches only when a feasible alternative remains better after full switch costs. Path labels are observed results, not plan constructors.
