# Task Card — IMP-EV-0096 Configuration-dependent model tool surface

## Identity

- Task ID: IMP-EV-0096
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0096 (ADOPT); owner label: Tool Runtime; subsystem: tool-runtime
- Qualification: QUAL-EV-0096 — Snapshot tool schemas across modes and verify denied/irrelevant tools absent.
- Evidence tier: real-system or production-equivalent

## Goal

Snapshot tool schemas across modes and verify denied/irrelevant tools absent.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was: every registered tool projected)
- production entry point: services/modbit-core/src/tools.rs visible_specs (support × policy); ListTools{task_id}; runtime projection
- proof: local_trusted vs review_isolated surfaces differ exactly by the tools the lease does not carry; shell tools absent without the broker

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
