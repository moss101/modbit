# Task Card — IMP-EV-0098 Typed tool result re-entry

## Identity

- Task ID: IMP-EV-0098
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0098 (ADOPT); owner label: Event Protocol; subsystem: domain-events
- Qualification: QUAL-EV-0098 — Provider replay preserves tool-call/result pairing across restart.
- Evidence tier: real-system or production-equivalent

## Goal

Provider replay preserves tool-call/result pairing across restart. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-core/src/runtime.rs rebuild() reconstructs the transcript from the log
- proof: after a Core restart the first resumed request carries every tool result paired with its call; no duplicate write

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
