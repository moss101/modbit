# Task Card — PX-016 Change strategy contract

## Identity

- Task ID: PX-016
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-016; owner: workspace-git (change engine) with the core-runtime harness; docs/28 §3
- Qualification: QUAL-PX-016 — real fixture task with a test harness: failing test written first, then the change; diffs are revision-bound ChangeTransactions with one concern each; a file outside the plan triggers a plan revision event; silent scope widening without a plan revision is rejected; a lockfile edited by hand rather than by its generator is flagged
- Evidence tier: real-system or production-equivalent

## Goal

Small revision-bound diffs, one concern per ChangeTransaction, tests first where a harness exists, explicit plan revision on scope change, generated files only via generators or with an explicit plan entry.

## Existing-code audit

- classification: COMPLETED in this batch (was PARTIAL: revision-bound ChangeTransactions, FileChanged per write and the DI-1/DI-2/DI-3 invariants existed; out-of-plan writes were only counted, and a planned lockfile edit raised nothing)
- production entry point: crates/core-runtime/src/harness.rs `HarnessState::check_write` and `write_targets` (a write to a path the current plan does not declare is refused `HARNESS_PLAN_REVISION_REQUIRED` before any effector; `plan.update` records `PlanRevised` with the scope delta); crates/verification/src/invariants.rs DI-2 FLAG for a generated file/lockfile changed by hand even with a plan entry; the change engine's per-write ChangeTransaction (M2.4) and FileChanged events (IMP-EV-0106)
- proof: on the real rust-cli fixture with a scripted model the failing test is written first (the TARGETED run reports it FAIL), then the one-concern fix (the next run reports PASS); four writes are four FileChanged events with strictly increasing workspace revisions; the README write outside the plan is refused and never reaches the effector; the plan revision names README.md and Cargo.lock as added with its reason; the hand-edited lockfile lands a DI-2 FLAG despite the plan entry

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening`
- `harness::tests::plan_gates_writes_and_completion_needs_clean_state` (check_write / write_targets)
- `diff_invariants_deny_test_weakening_and_flag_the_rest` (DI-2 flag with a plan entry)
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (its plan now declares the test file; DI-3 still denies the weakening)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
