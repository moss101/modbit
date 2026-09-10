# Task Card — PX-039 Repair policy defaults, reproduction-first and no-progress detection

## Identity

- Task ID: PX-039
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-039; owner: core-runtime; docs/28 §5 "Repair policy"
- Qualification: QUAL-PX-039 — seeded defect fixture: the first verification run reproduces the reported failure before any fix transaction; a fix proposed before reproduction is rejected while reproduction_required holds; an identical change_fingerprint is rejected and counted as an equivalent attempt; an oscillating change escalates; three consecutive turns with no new transaction, verification, retrieval, plan revision or question emit NoProgressDetected and escalate or move the task to Needs Attention; the Alpha defaults 2/6/1/true/3 are read from the versioned RepairPolicy, not hard-coded. Negative proof: a prompt cannot raise the bounds; an UNREPRODUCED failure cannot be silently treated as reproduced; a WORSENED attempt beyond max_worsened_before_escalation without escalation fails
- Evidence tier: real-system or production-equivalent

## Goal

The repair loop's discipline is one versioned policy object, reproduction comes before a fix, and a run that stops making progress says so.

## Existing-code audit

- classification: IMPLEMENTED in this batch (PX-018 delivered the attempt records and the per-signature and per-task bounds; this task adds the versioned defaults for reproduction and no-progress, the reproduction-first gate, oscillation detection and the docs/28 progress categories)
- production entry point: `crates/core-runtime/src/harness.rs` — `RepairPolicy` now carries `reproduction_required` and `max_consecutive_no_progress_turns` (Alpha 2/6/1/true/3) and `Budgets::default()` reads the no-progress bound from it; `goal_reports_failure`, `check_reproduction`, `record_reproduction`, `waive_reproduction`, and the oscillation branch of `conclude_attempt` (a change that returns the workspace to a state a prior attempt started from). `services/modbit-core/src/runtime.rs` — the mandatory BASELINE now runs before the harness gates so it is the run that decides reproduction; a failing command or test the agent runs also reproduces; `ReproductionRecorded` is appended for REPRODUCED, UNREPRODUCED and WAIVED; a write is refused `HARNESS_REPRODUCTION_REQUIRED` while the failure is unreproduced; progress is a transaction, a verification, a retrieval record, a plan revision or a question, so browsing does not reset the no-progress counter; domain `TaskEvent::ReproductionRecorded`
- proof: on a repository with no derivable suite the reported failure is UNREPRODUCED at the baseline and both fix attempts are refused; a plan revision that states the limitation records WAIVED and the write then lands exactly once. On the rust-cli fixture the baseline reproduces the failure before the first fix transaction (`ReproductionRecorded REPRODUCED` precedes the first `FileChanged` of the source file). Three turns of a non-progress tool emit `NoProgressDetected` with the policy's default of 3 turns and move the task to Needs Attention with `task.complete` never running

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_039_reproduction_first_is_enforced_and_no_progress_turns_escalate`
- `harness::tests::repair_policy_defaults_are_versioned_reproduction_gates_fixes_and_oscillation_escalates`
- `qual_px_018_repair_attempts_are_recorded_bounded_reverted_when_worsened_and_escalate_on_equivalence` (reproduction precedes the fix; equivalent hypothesis and WORSENED revert)
- `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review` (the agent reproduces with its own command before the fix)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
