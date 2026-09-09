# Task Card — IMP-EV-0044 End-to-end capability invariant

## Identity

- Task ID: IMP-EV-0044
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0044 (ADOPT); owner label: Capability Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0044 — Remove browser consumer; browser tool disappears rather than failing after model selects it.
- Evidence tier: real-system or production-equivalent

## Goal

Remove browser consumer; browser tool disappears rather than failing after model selects it.

## Existing-code audit

- classification: IMPLEMENTED in this batch; the browser consumer itself is deferred (REQ-EV-0088/0147)
- production entry point: runtime dispatch refuses TOOL_NOT_VISIBLE before any effector; projection excludes unsupported tools (the terminal broker stands in for the browser consumer, which does not exist yet)
- proof: the model naming shell.exec without a broker gets a typed refusal; no ToolCallProposed exists for it

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
