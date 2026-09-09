# Task Card — IMP-EV-0064 Typed UndoPlan

## Identity

- Task ID: IMP-EV-0064
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0064 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0064 — Undo created/deleted/modified files while preserving unrelated user changes.
- Evidence tier: real-system or production-equivalent

## Goal

Undo created/deleted/modified files while preserving unrelated user changes.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: services/modbit-core/src/undo.rs (UndoPlan of typed inverse actions from FileChanged events; UndoToolCall surface command; CLI change undo)
- proof: delete the created, restore the deleted, replace the modified; unrelated user edits untouched; undo lands its own FileChanged events

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0064_0065_typed_undo_restores_inverse_actions_and_a_user_edit_blocks_the_revert`
- `modbit-cli/smoke` :: `cli_drives_a_real_core_end_to_end`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
