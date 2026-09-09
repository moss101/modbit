# Task Card — IMP-EV-0079 Typed tools

## Identity

- Task ID: IMP-EV-0079
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0079 (ADOPT); owner label: Tool Runtime; subsystem: tool-runtime
- Qualification: QUAL-EV-0079 — Invalid alias/schema is repaired or rejected before effector.
- Evidence tier: real-system or production-equivalent

## Goal

Invalid alias/schema is repaired or rejected before effector. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/tools/src/pipeline.rs (typed pre-execute validation)
- proof: malformed and alias arguments never reach the effector; the refusal is typed on the log

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `qual_ev_0079_invalid_arguments_are_rejected_before_any_effector`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
