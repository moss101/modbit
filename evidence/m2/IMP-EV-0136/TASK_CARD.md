# Task Card — IMP-EV-0136 Permission modes

## Identity

- Task ID: IMP-EV-0136
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0136 (ADAPT); owner label: Policy Profiles; subsystem: effects-security
- Qualification: QUAL-EV-0136 — Mode switch requiring user action cannot be triggered by model tool call.
- Evidence tier: real-system or production-equivalent

## Goal

Mode switch requiring user action cannot be triggered by model tool call. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/policy/src/kernel.rs (profiles compile to monotonic policy; no mode-switch capability exists)
- proof: the tool registry exposes no policy or mode tool; the kernel denies unknown operations

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-policy/kernel` :: `qual_ev_0136_modes_compile_to_monotonic_policy_and_no_tool_switches_them`
- `modbit-core/surface_protocol` :: `qual_ev_0194_approvals_are_canonical_and_never_resolved_by_the_model`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
