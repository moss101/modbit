# Task Card — IMP-EV-0119 Goal mode

## Identity

- Task ID: IMP-EV-0119
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0119 (ADAPT); owner label: Task Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0119 — Model says done while acceptance fails; run remains incomplete.
- Evidence tier: real-system or production-equivalent

## Goal

Model says done while acceptance fails; run remains incomplete. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/core-runtime/src/harness.rs CompletionBlocked; COMPLETE_TOOL verification in services/modbit-core/src/runtime.rs
- proof: task.complete with failing acceptance is refused; the run stays open until the gate passes

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
