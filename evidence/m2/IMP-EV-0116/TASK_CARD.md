# Task Card — IMP-EV-0116 Minimal tool sets per agent

## Identity

- Task ID: IMP-EV-0116
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0116 (ADOPT); owner label: Tool Runtime; subsystem: tool-runtime
- Qualification: QUAL-EV-0116 — Tool-schema token benchmark vs eager all-tools baseline.
- Evidence tier: real-system or production-equivalent

## Goal

Tool-schema token benchmark vs eager all-tools baseline.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: runtime projection over visible_specs; benchmark inside the named test (wire shape, harness tools excluded)
- proof: projected schema bytes are below the eager projection of the host list (printed as QUAL-EV-0116 in the test log)

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
