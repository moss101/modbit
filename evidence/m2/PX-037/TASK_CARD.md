# Task Card — PX-037 Diff invariants including test-integrity detection

## Identity

- Task ID: PX-037
- Milestone: M2 (backlog batch 2e: qualification of the M2.7/M2.8 substrate)
- Requirement: REQ-PX-037 (ADOPT); owner label: workspace-git; subsystem: workspace-git
- Qualification: QUAL-PX-037
- Evidence tier: real-system or production-equivalent

## Goal

Real worktree: the Change Engine evaluates DI-1..DI-9 when a ChangeTransaction is proposed and the Verification Engine evaluates the whole diff at COMPLETION; deleting, skipping, weakening or rewriting an acceptance-named test is rejected as a ToolCallPolicyDecision with a DiffInvariantViolated event; a hand-edited lockfile and a dependency-manifest change without a plan entry are rejected; debug leftovers and formatting churn beyond the threshold are flagged and block the SelfReview until resolved or justified

## Existing-code audit

- classification: FOUND (M2.8) — qualification
- production entry point: crates/verification/src/invariants.rs DI-1..DI-9 evaluated per ChangeTransaction (transaction_invariants → DiffInvariantViolated, DENY refuses the write) and over the whole diff at COMPLETION
- proof: skip markers, removed assertions and rewrites of acceptance-named tests are DENY (DI-3) with DiffInvariantViolated on the log and the write refused before any effect; hand-edited lockfiles and manifests without a plan entry are DENY (DI-2/DI-7); debug leftovers and formatting churn are FLAGs; credentials DENY (DI-5)

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-verification/fixtures` :: `diff_invariants_deny_test_weakening_and_flag_the_rest`
- `modbit-core/surface_protocol` :: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`
- `modbit-verification/fixtures` :: `qual_ev_0071_secret_in_patch_is_denied_with_evidence`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34439622680.json`, `ci-run-34439622680-tests.log`: hosted CI run 34439622680 on fbc7657, green on macOS, Linux and Windows
