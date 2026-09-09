# Task Card — IMP-EV-0015 Deterministic edit match ladder

## Identity

- Task ID: IMP-EV-0015
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0015 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0015 — Ambiguous duplicated target must fail and leave worktree unchanged.
- Evidence tier: real-system or production-equivalent

## Goal

Ambiguous duplicated target must fail and leave worktree unchanged.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: only byte-range patches existed)
- production entry point: crates/workspace/src/service.rs locate + edit_by_match (exact → whitespace remap → ambiguity/no-match with a contextual suggestion; never a guess); change.apply op=edit
- proof: duplicated targets fail at either tier; bytes, revision and temp files untouched; a miss reports the closest line without applying it

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-workspace/real_fs` :: `qual_ev_0015_ambiguous_target_fails_and_leaves_the_worktree_unchanged`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
