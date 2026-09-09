# Task Card — IMP-EV-0045 Capability-oriented autonomous mode

## Identity

- Task ID: IMP-EV-0045
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0045 (ADOPT); owner label: Policy Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0045 — Autonomous run cannot request higher privilege than profile ceiling.
- Evidence tier: real-system or production-equivalent

## Goal

Autonomous run cannot request higher privilege than profile ceiling. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/policy/src/kernel.rs (profile ceilings, no-approval profiles)
- proof: local_autonomous is denied above its ceiling and cannot escalate through approval

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-policy/kernel` :: `qual_ev_0045_autonomous_profile_cannot_exceed_or_ask_past_its_ceiling`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
