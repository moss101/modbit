# Task Card — PX-032 Pre-change verification baseline and regression attribution

## Identity

- Task ID: PX-032
- Milestone: M2 (backlog batch 2e: qualification of the M2.7/M2.8 substrate)
- Requirement: REQ-PX-032 (ADOPT); owner label: verification; subsystem: verification
- Qualification: QUAL-PX-032
- Evidence tier: real-system or production-equivalent

## Goal

Real fixture with one pre-existing failing test and one test the task will break: a BASELINE run executes before the first write and records a VerificationBaseline; the pre-existing failure is labelled KNOWN_FAILING; at the COMPLETION run the newly broken test is attributed as a REGRESSION and blocks acceptance; a check the plan declared as an expected behavior change before the run is shown in Review as declared, not as a regression

## Existing-code audit

- classification: FOUND (M2.8) — qualification
- production entry point: BASELINE stage before the first write (services/modbit-core/src/runtime.rs), attribution in crates/verification/src/engine.rs (KNOWN_FAILING vs REGRESSION vs declared)
- proof: on the rust-cli fixture the pre-existing failure is KNOWN_FAILING at BASELINE, the test the task breaks is attributed REGRESSION at COMPLETION and blocks acceptance until fixed; a check declared as an expected change before the run is attributed as declared, not a regression

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-verification/fixtures` :: `cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
