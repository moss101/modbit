# Task Card — PX-017 Verification plan derivation contract

## Identity

- Task ID: PX-017
- Milestone: M2 (backlog batch 2e: qualification of the M2.7/M2.8 substrate)
- Requirement: REQ-PX-017 (ADOPT); owner label: verification; subsystem: verification
- Qualification: QUAL-PX-017
- Evidence tier: real-system or production-equivalent

## Goal

Real fixture: derived verification plan recorded before the first run with build, typecheck, targeted tests, diagnostics delta and diff invariants; agent-added checks appear; mandatory checks cannot be removed

## Existing-code audit

- classification: FOUND (M2.8) — qualification
- production entry point: crates/verification/src/plan.rs derive() (build/typecheck/suite/targeted commands, mandatory flags, acceptance-named checks, declared changes) recorded by the runtime before the first run (verification_plan_ref)
- proof: the derived plan is recorded before the BASELINE run; commands carry mandatory flags that attribution enforces (a NEW_FAILING or REGRESSION on a mandatory check blocks acceptance); diff invariants are evaluated per transaction and at COMPLETION; the plan's declared changes are honoured by attribution

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-verification/fixtures` :: `cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions`
- `modbit-verification/fixtures` :: `pytest_junit_and_configured_command_adapters`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
