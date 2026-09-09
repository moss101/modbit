# Task Card — IMP-EV-0014 Canonical edit transaction

## Identity

- Task ID: IMP-EV-0014
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0014 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0014 — Concurrent user edit causes precondition failure without data loss.
- Evidence tier: real-system or production-equivalent

## Goal

Concurrent user edit causes precondition failure without data loss. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/workspace/src (revision-bound change.apply preconditions)
- proof: a stale revision precondition is refused before any byte is written; the user's edit survives

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-workspace/real_fs` :: `stale_preconditions_are_rejected_and_nothing_is_written`
- `modbit-workspace/real_fs` :: `replacement_is_atomic_when_the_rename_fails`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
