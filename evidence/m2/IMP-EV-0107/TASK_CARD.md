# Task Card — IMP-EV-0107 Failure output used for repair

## Identity

- Task ID: IMP-EV-0107
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0107 (ADOPT); owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0107 — Seed failing test; model receives bounded failure and repairs without losing raw log.
- Evidence tier: real-system or production-equivalent

## Goal

Seed failing test; model receives bounded failure and repairs without losing raw log. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: services/modbit-core/src/runtime.rs run_verification → bounded CheckSummary to the model; raw_output_ref retained
- proof: the model sees the bounded failure summary, repairs, and the raw log stays retrievable by digest

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-core/surface_protocol` :: `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
