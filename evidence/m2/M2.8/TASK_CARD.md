# Task Card — M2.8 Verification engine build/test checks

## Identity

- Task ID: M2.8
- Milestone: M2
- Canonical owner: verification (`crates/verification`); the Core binds it to the process broker and the agent loop
- Requirement IDs: REQ-EV-0018 (regression-only attribution), REQ-EV-0068 (verifier crash / unknown outcome is INDETERMINATE, never success), REQ-EV-0070 (baseline before mutation), REQ-EV-0107 (bounded failure evidence with the raw log retained); docs/64 §1–§4, §8; docs/28 §4; docs/43 M2.8 acceptance.
- Risk class: high (decides acceptance of real changes)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

A derived verification plan recorded before the first run; BASELINE before the first write (KNOWN_FAILING labelled, flaky checks pre-quarantined), TARGETED runs from the agent's `verify.run`, and a mandatory COMPLETION run on `task.complete` with regression attribution against BASELINE and whole-diff invariants; normalized `TestReport`/`CheckResult` from real runners (cargo libtest, vitest JSON, pytest JUnit, configured command with HEURISTIC confidence); `failure_signature` derived only from normalized results; the one-rerun flake protocol with task-scoped quarantine; diff invariants DI-1..DI-9 evaluated per `change.apply` (DENY refuses before any effect) and over the whole diff at COMPLETION (FLAG blocks the SelfReview).

## Non-goals

- Impact-based selection (PX-035, M3); TARGETED runs execute the derived plan's commands.
- pytest runs on the hosted runners (no interpreter with pytest); the JUnit adapter is proven on a recorded report.
- `mandatory_flaky_passes` accumulation across runs; declared expected changes reach the engine through the plan artifact but the agent-facing `plan.update` schema does not yet expose them.
- AST-based DI-3 detection for Tier A languages (conservative text rules apply, FLAG when unsure).

## Required reading

`AGENTS.md`, `docs/28`, `docs/50`, `docs/64`, `docs/76`, `tests/fixtures/repos/*/README.md`.

## Existing-code audit

- classification: NOT-FOUND (`crates/verification` was an M0.1 shell; `test.run` returned a HEURISTIC configured-command report only)
- production entry point: `modbit_verification::{derive, VerificationEngine::run_stage, attribute_against, evaluate_file, evaluate_diff}`; Core `runtime.rs` (BASELINE before first write, `verify.run`, COMPLETION on `task.complete`, per-transaction invariants); `verify.rs` broker runner
- real effector/storage boundary: real `cargo test` / `node vitest` processes through `modbit-execd`; reports and raw output as content-addressed objects; `verification_runs`, `check_results`, `flaky_checks` tables (schema v6) derived from `VerificationBaselineRecorded`, `VerificationRunRecorded`, `FlakyCheckQuarantined`, `RegressionAttributed`, `DiffInvariantViolated`

## Invariants

- No write before BASELINE; the plan object is recorded before the first run.
- A check failing at BASELINE is KNOWN_FAILING and never attributed; PASS→FAIL at COMPLETION is a REGRESSION that blocks acceptance unless declared before the run.
- Only the rerun protocol labels FLAKY; FLAKY and UNKNOWN never form a failure signature; UNKNOWN is INDETERMINATE.
- DENY invariants (DI-1/2/5/7/8/9 and DI-3 on acceptance-named or baseline-failing tests) refuse the transaction with `DiffInvariantViolated`; FLAG findings block completion until resolved or declared in the plan.
- The model receives failing CheckResults first with locations, error classes and raw output references; the full raw output is retained.
- Verification artifacts (reporter files, fixture state) never enter the candidate tree.

## Verification

- `crates/verification/tests/fixtures.rs`: real cargo runs on `rust-cli` (structured parse, KNOWN_FAILING, FLAKY via isolated rerun, stable signatures across runs, REGRESSION / COLLATERAL_FIX / KNOWN_FAILING / DECLARED_CHANGE attribution), real vitest runs on `ts-webapp`, pytest JUnit and configured-command adapters (UNKNOWN is INDETERMINATE), DI-1..DI-9 DENY/FLAG rules
- Core: `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (BASELINE precedes the first write; DI-3 DENY on weakening the acceptance test; TARGETED failing checks reach the model; first COMPLETION refused on REGRESSION; second COMPLETION clean → ReadyForReview; suite still FAILED on the pre-existing failure without blocking)
- E2E: hosted CI on macOS/Linux/Windows (the rust job now installs the fixture's vitest)

## Completion evidence

See `evidence.json`.
