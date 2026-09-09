# Task Card — IMP-EV-0036 Per-hunk diff review

## Identity

- Task ID: IMP-EV-0036
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0036 (ADOPT); owner label: Workspace UI + Change Engine; subsystem: desktop
- Qualification: QUAL-EV-0036 — Reject one hunk and accept another; resulting Git diff matches user choices exactly.
- Evidence tier: real-system or production-equivalent

## Goal

Reject one hunk and accept another; resulting Git diff matches user choices exactly. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-core/src/review.rs decide(); crates/git/src/diff.rs apply_selected; apps/desktop Review
- proof: the committed diff equals the accepted hunks only; rejected hunks are left uncommitted

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_9_review_surface_applies_per_hunk_decisions_and_commits`
- `desktop-e2e/review.spec.ts` :: `review surface: per-hunk decision commits exactly the user's choices`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
