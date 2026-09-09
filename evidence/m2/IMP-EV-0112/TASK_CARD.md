# Task Card — IMP-EV-0112 Provider/model/effort/service-tier envelope

## Identity

- Task ID: IMP-EV-0112
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0112 (ADAPT); owner label: Model Gateway; subsystem: model-gateway
- Qualification: QUAL-EV-0112 — Routing record shows requested vs resolved values and policy reason.
- Evidence tier: real-system or production-equivalent

## Goal

Routing record shows requested vs resolved values and policy reason. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/providers/src/gateway.rs RouteRecord; carried on ModelUsageRecorded.route by the runtime
- proof: requested and resolved model, effort and tier plus the policy reason; provider-neutral

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-providers/conformance` :: `qual_ev_0112_route_record_shows_requested_vs_resolved_values_and_policy_reason`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
