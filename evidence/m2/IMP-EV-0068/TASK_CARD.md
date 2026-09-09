# Task Card — IMP-EV-0068 VerificationPlane

## Identity

- Task ID: IMP-EV-0068
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0068 (ADOPT); owner label: Verification Plane; subsystem: verification
- Qualification: QUAL-EV-0068 — Verifier crash yields INDETERMINATE, never success.
- Evidence tier: real-system or production-equivalent

## Goal

Verifier crash yields INDETERMINATE, never success. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/verification/src/engine.rs (Unknown → INDETERMINATE blocks acceptance)
- proof: a verifier killed by SIGKILL or absent yields no PASS; attribution is inconclusive and blocks

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `qual_ev_0068_verifier_crash_is_indeterminate_never_success`
- `modbit-verification/fixtures` :: `cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
