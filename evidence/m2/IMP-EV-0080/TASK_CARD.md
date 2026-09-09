# Task Card — IMP-EV-0080 Policy before execution

## Identity

- Task ID: IMP-EV-0080
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0080 (ADOPT); owner label: Capability Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0080 — Prompt injection asking to bypass policy fails.
- Evidence tier: real-system or production-equivalent

## Goal

Prompt injection asking to bypass policy fails. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/policy/src/kernel.rs decides before crates/tools/src/pipeline.rs executes
- proof: argument text claiming authorization changes nothing; the lease is the authority

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `qual_ev_0239_0080_denial_is_monotonic_and_argument_text_cannot_bypass_policy`
- `modbit-policy/kernel` :: `lease_is_authority_not_tool_name`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
