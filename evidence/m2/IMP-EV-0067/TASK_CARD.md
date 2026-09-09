# Task Card — IMP-EV-0067 MergeTransaction

## Identity

- Task ID: IMP-EV-0067
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0067 (ADOPT); owner label: Change Engine; subsystem: workspace-git
- Qualification: QUAL-EV-0067 — Injected conflict + failed test leaves merge transaction inspectable and recoverable.
- Evidence tier: real-system or production-equivalent

## Goal

Injected conflict + failed test leaves merge transaction inspectable and recoverable. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/git/src (MergeTransaction)
- proof: a real merge conflict is exposed as data and the transaction can be abandoned cleanly

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-git/real_git` :: `qual_ev_0067_merge_transaction_exposes_injected_conflict_and_is_recoverable`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
