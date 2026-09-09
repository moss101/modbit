# Task Card — IMP-EV-0069 Optional adaptive LLM verification

## Identity

- Task ID: IMP-EV-0069
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0069 (EXPERIMENT); owner label: Experiments — not production commitments; subsystem: verification
- Qualification: QUAL-EV-0069 — Disabled configuration emits zero verifier model calls.
- Evidence tier: real-system or production-equivalent

## Goal

Disabled configuration emits zero verifier model calls. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/verification (no provider dependency; deterministic gates only)
- proof: EXPERIMENT off by default: the verification crate has no model port, so zero calls by construction

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `qual_ev_0069_disabled_semantic_verification_makes_zero_model_calls`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
