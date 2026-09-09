# Task Card — IMP-EV-0031 Enterprise model policy

## Identity

- Task ID: IMP-EV-0031
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0031 (ADOPT); owner label: Policy Kernel; subsystem: effects-security
- Qualification: QUAL-EV-0031 — Blocked provider remains unavailable despite task/profile request.
- Evidence tier: real-system or production-equivalent

## Goal

Blocked provider remains unavailable despite task/profile request.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: crates/providers/src/gateway.rs OrgModelPolicy (MODBIT_MODEL_POLICY) evaluated in route() first; ListModels blocked_by_policy; ProbeModel POLICY_BLOCKED
- proof: a blocked provider/model/endpoint is refused before any bytes leave, whatever effort/tier/model the request names

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-providers/conformance` :: `qual_ev_0031_org_policy_blocks_the_provider_despite_the_request`
- `modbit-core/surface_protocol` :: `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
