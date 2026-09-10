# Task Card — PX-018 Bounded evidence-driven repair loop with RepairAttempt records

## Identity

- Task ID: PX-018
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-018; owner: core-runtime; docs/28 §5 (repair loop and repair policy)
- Qualification: QUAL-PX-018 — seeded failing fixture: each repair attempt records failure signature, hypothesis, evidence, intended fix, change ref and verification result before the next run; a repeated equivalent hypothesis triggers escalation or Needs Attention with the attempt history; attempt bounds from policy are enforced; a change after a failed verification without a RepairAttempt is rejected; an equivalent hypothesis executed twice is impossible; bound exhaustion never truncates silently; WORSENED attempts are reverted or justified
- Evidence tier: real-system or production-equivalent

## Goal

RepairAttempt record per attempt (failure signature, hypothesis, evidence, intended fix, change ref, verification result, outcome); equivalent repeated hypotheses escalate; policy bounds per failure signature and per task; WORSENED attempts reverted.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: verification failures were open signatures, but nothing bound a change to a recorded attempt, detected repeated hypotheses, bounded attempts or reverted a worsening change)
- production entry point: crates/core-runtime/src/harness.rs (`REPAIR_TOOL`, `RepairPolicy` — Alpha defaults 2 per signature, 6 per task, 1 WORSENED before escalation —, `RepairAttempt`, `hypothesis_fingerprint`, `check_repair_gate`, `start_attempt`, `conclude_attempt`); services/modbit-core/src/runtime.rs (`repair.attempt` harness tool, the write gate `HARNESS_REPAIR_ATTEMPT_REQUIRED`, `conclude_repair` after every TARGETED run with the change fingerprint from the attempt's FileChanged records, the WORSENED revert through `undo::plan`/`apply`, `RepairEscalated` with the history object, `LoopEnd::NeedsAttention` → `TaskWaiting` + `TaskNeedsAttention`, rebuild from `RepairAttemptRecorded`/`RepairAttemptConcluded`); domain events `RepairAttemptRecorded`, `RepairAttemptConcluded`, `RepairEscalated`
- proof: on the real rust-cli fixture a change after the failed TARGETED run is refused until `repair.attempt` records signature, hypothesis, fingerprint, evidence and intended fix (the record precedes the change in the log); the wrong change is concluded WORSENED with its change refs and fingerprint and reverted on disk (an `undo` FileChanged follows); the same hypothesis in other words is refused and escalates: `RepairEscalated` carries the reason and the history object, the task is Waiting with `TaskNeedsAttention` "repair escalated …", and `task.complete` never runs; the harness unit test covers the per-signature and per-task bounds, the equivalent-change (no progress) escalation and the WORSENED threshold

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_018_repair_attempts_are_recorded_bounded_reverted_when_worsened_and_escalate_on_equivalence`
- `harness::tests::repair_loop_records_attempts_and_escalates_on_equivalence_bounds_and_worsening`
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` and `qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening` (their fixes now record an attempt first)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
