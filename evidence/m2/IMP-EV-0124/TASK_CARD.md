# Task Card — IMP-EV-0124 Clone branch into new session

## Identity

- Task ID: IMP-EV-0124
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0124 (ADOPT); owner label: Workspace Fabric; subsystem: workspace-git
- Qualification: QUAL-EV-0124 — Two sessions modify separately and merge transaction detects conflict.
- Evidence tier: real-system or production-equivalent

## Goal

Two sessions modify separately and merge transaction detects conflict. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/git/src (worktree lineage + MergeTransaction)
- proof: branches cloned from one base diverge and the merge transaction surfaces the conflict

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-git/real_git` :: `qual_ev_0125_0124_worktrees_isolate_parallel_writes_and_carry_lineage`
- `modbit-git/real_git` :: `qual_ev_0067_merge_transaction_exposes_injected_conflict_and_is_recoverable`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
