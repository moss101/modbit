# Task Card — IMP-EV-0026 Layered terminal output economics

## Identity

- Task ID: IMP-EV-0026
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0026 (ADOPT); owner label: Terminal Broker; subsystem: terminal
- Qualification: QUAL-EV-0026 — Noisy build shows token reduction while raw output digest remains complete.
- Evidence tier: real-system or production-equivalent

## Goal

Noisy build shows token reduction while raw output digest remains complete. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/tools/src/pipeline.rs bounded structured output + OutputRef; execd streaming
- proof: 4000 lines of build noise reach the model as a < 8 KB view; the full raw output is retained and its digest matches

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `shell_exec_and_test_run_go_through_the_real_broker`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
