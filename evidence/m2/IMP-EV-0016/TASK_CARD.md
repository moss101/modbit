# Task Card — IMP-EV-0016 Sequential multi-edit transaction

## Identity

- Task ID: IMP-EV-0016
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0016 (ADAPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0016 — Injected failure at edit N rolls back transaction or emits explicit partial state by contract.
- Evidence tier: real-system or production-equivalent

## Goal

Injected failure at edit N rolls back transaction or emits explicit partial state by contract.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: crates/workspace/src/service.rs apply_transaction; change.batch tool; Error::StepFailed{step, rolled_back, restored, unrestored}
- proof: a stale precondition at step 3 restores steps 0..2 byte-for-byte and names the step; an unrestorable path would be reported as explicit partial state

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-workspace/real_fs` :: `qual_ev_0016_failed_step_rolls_back_earlier_steps_and_reports_the_step`
- `modbit-core/surface_protocol` :: `qual_ev_0064_0065_typed_undo_restores_inverse_actions_and_a_user_edit_blocks_the_revert`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
