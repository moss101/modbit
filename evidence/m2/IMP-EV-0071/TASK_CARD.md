# Task Card — IMP-EV-0071 PatchPolicyGate

## Identity

- Task ID: IMP-EV-0071
- Milestone: M2 (backlog batch 2)
- Requirement: REQ-EV-0071 (ADAPT); owner label: Verification Plane; subsystem: verification
- Qualification: QUAL-EV-0071 — Seed secret in patch; merge blocked with evidence.
- Evidence tier: real-system or production-equivalent

## Goal

Seed secret in patch; merge blocked with evidence.

## Existing-code audit

- classification: FOUND (DI-5 existed); named qualification added
- production entry point: crates/verification/src/invariants.rs DI-5 (Deny) evaluated per transaction in services/modbit-core/src/runtime.rs transaction_invariants and at COMPLETION
- proof: a credential-like token in a patch is DI-5 DENY with evidence naming the file and never echoing the secret; the runtime refuses DENY at the TRANSACTION stage (M2.8)

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `qual_ev_0071_secret_in_patch_is_denied_with_evidence`
- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34413048026.json`, `ci-run-34413048026-tests.log`: hosted CI run 34413048026 on a4a456d, green on macOS, Linux and Windows
