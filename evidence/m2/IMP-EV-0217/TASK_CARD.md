# Task Card — IMP-EV-0217 Explicit built-in tool families

## Identity

- Task ID: IMP-EV-0217
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0217 (ADAPT); owner label: Tool Runtime; subsystem: tool-runtime
- Qualification: QUAL-EV-0217 — Compatibility matrix has canonical owner/effect/test for each source capability.
- Evidence tier: real-system or production-equivalent

## Goal

Compatibility matrix has canonical owner/effect/test for each source capability.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: crates/tools/tool-matrix.json checked against the registry
- proof: every registered tool has a row with owner, effect equal to the registered class, capabilities, source behaviors and an existing qualifying test; no stale rows

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
