# Task Card — IMP-EV-0018 Regression-only diagnostics comparison

## Identity

- Task ID: IMP-EV-0018
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0018 (ADOPT); owner label: Verification Plane; subsystem: verification
- Qualification: QUAL-EV-0018 — Fixture with pre-existing errors proves baseline issues are not blamed on change.
- Evidence tier: real-system or production-equivalent

## Goal

Fixture with pre-existing errors proves baseline issues are not blamed on change. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/verification/src/engine.rs attribute(); BASELINE stage before the first write in services/modbit-core/src/runtime.rs
- proof: KNOWN_FAILING at baseline is never a REGRESSION; only new failures are attributed

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions`
- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
