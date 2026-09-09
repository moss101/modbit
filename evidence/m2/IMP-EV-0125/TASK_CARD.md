# Task Card — IMP-EV-0125 Git worktree commands

## Identity

- Task ID: IMP-EV-0125
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0125 (ADOPT); owner label: Worktree Manager; subsystem: workspace-git
- Qualification: QUAL-EV-0125 — Parallel E2E verifies no cross-worktree writes.
- Evidence tier: real-system or production-equivalent

## Goal

Parallel E2E verifies no cross-worktree writes. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/git/src (worktree create/close) and git.worktree tools
- proof: two worktrees written in parallel never see each other's changes

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-git/real_git` :: `qual_ev_0125_0124_worktrees_isolate_parallel_writes_and_carry_lineage`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
