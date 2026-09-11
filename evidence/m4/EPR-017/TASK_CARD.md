# Task Card — EPR-017 Separate assurance classification and acceptance

## Identity

- Task ID: EPR-017
- Milestone: M4 Durable recovery spine (P0); phase 2-3; prerequisites EPR-008, M4.6
- Requirement: REQ-EPR-017 (related REQ-EV-0014, REQ-EV-0029); owner: verification; subsystem: `crates/verification` (gate), Core wiring in `services/modbit-core`
- Qualification: `QUAL-EPR-017` / EPR-E2E-017 — run real repository build/type/test/security/API evidence through the verification route; high assurance with missing review stays INCONCLUSIVE, deterministic failure rejects, complete current evidence accepts; record independent risk and gate versions/refs across restart. EPR-FI-017 — forge PASS evidence on a critical surface, omit human proof, stale revision or missing slot: no ACCEPT or branch generation; mutate risk/acceptance conflation and prove independent tests fail.
- Evidence tier: real-system (four real tasks on the real Core: a low-risk candidate accepted and completed across a restart; a critical-surface candidate held INCONCLUSIVE until the user's decision; a regression rejected; a worktree moved after the completion run refused at accept)

## Goal

Consume the policy-owned RealizedRisk and independently materialize `AcceptanceGateResult` (required assurance, evidence, verdict, missing evidence). Static checks precede review. ACCEPT/REJECT/INCONCLUSIVE activate only precompiled valid slots or safe stop; correct tests never erase mandatory review/human requirements.

## Existing-code audit

- classification: PARTIAL before this task. The completion handshake decided by regression attribution and diff invariants inside the harness, with no typed gate result, no required-assurance input, no independence from the risk (there was none), and the user's accept recorded a decision without checking the evidence still held at that revision. First missing link: no `AcceptanceGateResult` anywhere.
- production entry points:
  - `crates/verification/src/gate.rs` — `RequiredAssurance`, `GateInput` (COMPLETION run with checks and regression attribution, invariants, review decisions), `evaluate` → `AcceptanceGateResult { gate_version, plan_id, leg_id, candidate_revision, realized_risk_ref, required_assurance, evidence, verdict, missing_evidence, reject_reasons, evidence_refs, policy_version, risk_version }` (capability `verification.acceptance_gate`). Rules: failed checks the baseline did not already fail, DENY invariants, forbidden effects and a RETURN reject; missing, stale, cancelled or timed-out mandatory evidence — and a missing risk record — are INCONCLUSIVE; a review obligation is discharged by a user's or an independent reviewer's ACCEPT at the same revision, a human obligation only by a user's; ACCEPT requires everything present, current and passing.
  - `services/modbit-core/src/gate.rs` — gathers the evidence off the log (the latest COMPLETION `VerificationRunRecorded`, `RegressionAttributed`, `DiffInvariantViolated`, `ReviewDecisionRecorded`, the run's admitted plan and activated slot), builds the obligation from the policy minimum plus the latest realized risk, evaluates, stores the result object and emits `RunEvent::AcceptanceGateEvaluated` (valid on a completed run).
  - `services/modbit-core/src/runtime.rs` — at every COMPLETION run, after the risk: the run's records land, the gate is evaluated (trigger `COMPLETION_RUN`), the verdict goes into the observation and `harness_state.acceptance`.
  - `services/modbit-core/src/review.rs` — an ACCEPT is evaluated first with the decision counted as evidence at the current revision; not ACCEPT → `ACCEPTANCE_NOT_MET` naming the verdict, reject reasons and missing evidence, nothing recorded; ACCEPT → the decision, the gate's ACCEPT (trigger `REVIEW_DECISION`) and `TaskCompleted`.
  - `GetTaskAssurance.acceptance` (`AcceptanceGateView`); CLI `task assurance` prints the gate beside the risk.
- proof: (a) a coding task with a passing check completes to review with the gate ACCEPT at the completion run (tests PASS, invariants PASS, plan and initial slot named, gate `gate-1`, risk `risk-rules-1`, the risk ref equal to the derived record's); after a hard kill and restart the same gate ref and offset read back; the user's accept completes the task and records a second ACCEPT with trigger `REVIEW_DECISION` on the same risk ref. (b) The same passing evidence on an auth module is INCONCLUSIVE with `missing_evidence` = independent_review, human_decision and HIGH_ASSURANCE required; the user's accept discharges both (human_decision PASS, independent_review PASS) and completes the task. (c) A configured check that passed at baseline and fails at the candidate: the harness refuses completion on the REGRESSION and the gate on the log is REJECT with the failed test as evidence; the model's next turn carries the verdict; no review, no completion. (d) A direct write after the completion run moves the worktree: the accept is refused `ACCEPTANCE_NOT_MET` (INCONCLUSIVE, tests stale), the task stays in review, no decision and no completion are recorded, no branch opens. Unit tests pin: complete current evidence accepts; passing tests never erase a review or human obligation; a reviewer's accept does not discharge the human's; deterministic failure rejects and precedes review; stale, missing and incomplete evidence cannot accept; forbidden effects, DENY invariants and RETURN reject; a policy-required check must be present and passing.

## Limitations

- The only reviewer today is the user; the independent model reviewer and its precompiled slot (EPR-006/EPR-018, M5) will add `independent_reviewer` decisions the gate already accepts for the review obligation.
- Security and API-compatibility evidence kinds are recognised from check kinds but no runner produces them yet; they are absent, not faked.
- A gate REJECT at the completion run is recorded on the log and shown to the model; the harness's own refusal (regression attribution, invariants) is what blocks the proposal.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_017_acceptance_is_evidence_at_the_revision_and_never_erases_a_human_obligation` (services/modbit-core, real Core)
- Unit (crates/verification): `complete_current_evidence_accepts_a_standard_candidate`, `passing_tests_never_erase_a_review_or_human_obligation`, `deterministic_failure_rejects_and_precedes_review`, `stale_missing_or_incomplete_evidence_cannot_accept`, `forbidden_effects_deny_invariants_and_returns_reject`, `a_policy_required_check_must_be_present_and_passing`
- Regression: `m2_9_review_surface_applies_per_hunk_decisions_and_commits` (accept through the gate), `qual_epr_008_factual_risk_stays_strict_despite_passing_tests_and_stops_safely_unattended`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
