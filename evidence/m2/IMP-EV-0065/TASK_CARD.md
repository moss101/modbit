# Task Card — IMP-EV-0065 Optimistic-concurrency revert

## Identity

- Task ID: IMP-EV-0065
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0065 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0065 — User edit after agent change blocks destructive revert.
- Evidence tier: real-system or production-equivalent

## Goal

User edit after agent change blocks destructive revert.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: services/modbit-core/src/undo.rs apply(): every path must still carry its post-edit hash (or be absent) before anything is written
- proof: a user edit after the agent's change refuses the revert with USER_EDITED and writes nothing

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0064_0065_typed_undo_restores_inverse_actions_and_a_user_edit_blocks_the_revert`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
