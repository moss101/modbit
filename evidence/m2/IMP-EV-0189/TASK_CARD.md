# Task Card — IMP-EV-0189 Model vision/agent/modalities metadata

## Identity

- Task ID: IMP-EV-0189
- Milestone: M2 (backlog batch 1: qualification of substrate delivered by M2.1–M2.10)
- Requirement: REQ-EV-0189 (ADOPT); owner label: Model Gateway; subsystem: model-gateway
- Qualification: QUAL-EV-0189 — Routing refuses unsupported media model and selects eligible endpoint.
- Evidence tier: real-system or production-equivalent

## Goal

Routing refuses unsupported media model and selects eligible endpoint. The substrate for this requirement landed in the M2 milestone tasks; this card records the named qualification proof against the real substrate (real processes, real git, real SQLite, real Core over the socket) on hosted CI.

## Existing-code audit

- classification: FOUND (delivered by the M2 milestone tasks)
- production entry point: crates/providers/src/gateway.rs (modalities in ModelInfo)
- proof: an image request to a text-only model is refused with a typed reason and routed to a vision-capable endpoint

## Verification

Named tests (crate/test-binary :: test name), all run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-providers/conformance` :: `qual_ev_0028_0189_capability_catalog_refuses_mismatched_models_before_dispatch`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34409173853.json`, `ci-run-34409173853-tests.log`: hosted CI run 34409173853 on 7bdf76d, green on macOS, Linux and Windows
