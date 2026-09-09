# Task Card — IMP-EV-0133 Tool visibility conditional on host support

## Identity

- Task ID: IMP-EV-0133
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0133 (ADOPT); owner label: Capability Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0133 — Disable consumer adapter and verify schema disappears.
- Evidence tier: real-system or production-equivalent

## Goal

Disable consumer adapter and verify schema disappears.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: services/modbit-core/src/tools.rs visible_specs: shell-backed tools require the terminal broker consumer
- proof: a Core started without the broker binary advertises no shell.exec/test.run to clients or the model

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
