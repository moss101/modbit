# Task Card — PX-036 Flaky-check detection, rerun protocol and quarantine

## Identity

- Task ID: PX-036
- Milestone: M2 (backlog batch 2e)
- Requirement: REQ-PX-036 (ADOPT); owner label: verification; subsystem: verification
- Qualification: QUAL-PX-036
- Evidence tier: real-system or production-equivalent

## Goal

Fixture with a seeded flaky test: a failure triggers exactly one isolated rerun at the same revision and environment digest; the check is labelled FLAKY with both run references, excluded from failure_signature derivation, shown in Review and the SelfReview, and quarantined only for the task and revision range; a flaky mandatory check makes acceptance INCONCLUSIVE unless three consecutive isolated passes are obtained within budget; a check flaky at BASELINE is pre-quarantined

## Existing-code audit

- classification: FOUND (M2.8 rerun protocol) + IMPLEMENTED (the mandatory three-consecutive-pass rule was a policy default without a code path)
- production entry point: crates/verification/src/engine.rs run_stage flake protocol (VerificationPolicy.flake_rerun, mandatory_flaky_passes); FlakyCheckQuarantined on the run; review bundle quarantines
- proof: a seeded flaky test fails once, is rerun in isolation at the same revision and environment digest and, because its suite is mandatory, must pass three consecutive isolated reruns before it is labelled FLAKY and quarantined for the task and revision (both run references recorded, excluded from failure signatures, shown in Review and the self-review); a FLAKY attribution makes acceptance INCONCLUSIVE (blocks) at COMPLETION

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions`
- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34439622680.json`, `ci-run-34439622680-tests.log`: hosted CI run 34439622680 on fbc7657, green on macOS, Linux and Windows
