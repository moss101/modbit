# Task Card — IMP-EV-0239 Typed tool pre/execute/post pipeline

## Identity

- Task ID: IMP-EV-0239
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0239 (ADAPT); owner label: Tool Runtime; subsystem: tool-runtime
- Qualification: QUAL-EV-0239 — Hook tries to override deny after guard; execution remains denied.
- Evidence tier: real-system or production-equivalent

## Goal

Hook tries to override deny after guard; execution remains denied. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/tools/src/pipeline.rs (pre/execute/post; denial is monotonic)
- proof: no post-guard stage can flip a DENY; the effector is never invoked

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `qual_ev_0239_0080_denial_is_monotonic_and_argument_text_cannot_bypass_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
