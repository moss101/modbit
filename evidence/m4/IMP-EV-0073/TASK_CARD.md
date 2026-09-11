# Task Card — IMP-EV-0073 Typed failure/recovery/operator diagnostics

## Identity

- Task ID: IMP-EV-0073
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0073; owner label: Reliability Layer; subsystem: durability (core-runtime classifier, Core wiring, surface)
- Qualification: `QUAL-EV-0073` — Fault injection verifies no generic success on timeout/corrupt state.
- Evidence tier: real-system (the real Core, a real command that times out, a real object whose bytes no longer hash to their name, a real provider endpoint nobody answers)

## Goal

Failures carry class, retryability, user action, evidence and recovery path — to the model in its observation, to the log in the attention event, to the operator on the status surface — never a bare status line or a generic success.

## Existing-code audit

- classification: PARTIAL before this batch. Tool failures reached the model as `status:` plus an error code; `TaskNeedsAttention` carried a prose reason; `TaskStatus` reported state and wait reason only; a corrupt object read through `artifact.range` was reported as `NO_SUCH_ARTIFACT` (missing), blurring corrupt state into a missing artifact.
- production entry points:
  - `crates/domain/src/failure.rs` — `FailureClass` (`TIMEOUT`, `INFRASTRUCTURE`, `APPLICATION`, `POLICY`, `APPROVAL`, `CORRUPT_STATE`, `UNKNOWN_OUTCOME`, `LEASE`, `PROVIDER`, `BUDGET`, `HARNESS`, `CANCELLED`, `INVALID`) and `FailureDiagnostic { class, code, retryable, user_action, recovery_path, evidence_refs, features, detail }` with `render()`; `TaskEvent::TaskNeedsAttention { reason, diagnostic }`.
  - `crates/core-runtime/src/diagnostics.rs` — `classify(&FailureSource)`: one deterministic classifier over tool results (status, code, timed out, cancelled), provider codes, store errors, budgets, loop boundaries, restart boundaries and lost leases (capability `failure.diagnostics`).
  - `services/modbit-core/src/runtime.rs` — every non-success observation (tool failure, failed check, dispatch error, cancellation, denial) ends with the rendered diagnosis; every attention event (provider failure, budget, question, repair escalation, scope fail-closed, no progress, lost lease, restart boundary) carries it typed.
  - `services/modbit-core/src/tools.rs`, `crates/tools/src/direct.rs` — a digest mismatch on a stored object is `OBJECT_MISMATCH` (infrastructure), told apart from `NO_SUCH_ARTIFACT`; dispatch errors carry the store's own code (`STALE_LEASE`, `INTEGRITY`, …).
  - `crates/event-store/src/store.rs` `latest_attention`; `services/modbit-core/src/server.rs` `GetTaskStatus` → `TaskStatus { attention_reason, failure_class, failure_code, retryable, user_action, recovery_path, evidence_refs, diagnostic_features }`; `apps/cli` prints the diagnosis under `task status`.
- proof: a `shell.exec` that cannot finish inside its timeout reaches the model as `error_code: TIMEOUT` with `failure_class: TIMEOUT`, `retryable: true` and a recovery line, never `status: SUCCESS`, and lands as `ToolCallFailed` with the result as evidence; an `artifact.range` of a planted object whose bytes do not match its digest is `INFRAFAILURE` / `OBJECT_MISMATCH` with `failure_class: CORRUPT_STATE`, `retryable: false`, a user action and the cause named; a passing command carries no diagnosis; a run against a provider endpoint nobody answers suspends `Waiting/Provider` and `TaskStatus` reports `PROVIDER`, retryable, the endpoint to check and the resume path, the same diagnosis typed in the `TaskNeedsAttention` payload.

## Limitations

- The classifier keys on the typed status and code the source reports; a tool that reports a failure without a code is `APPLICATION / FAILED`, retryable.
- The desktop renders the wire fields but no dedicated attention panel yet (M5 surface work); the CLI prints them.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0073_fault_injection_never_reports_a_generic_success` (services/modbit-core, real Core)
- `qual_ev_0073_every_diagnosis_names_class_retryability_action_evidence_and_recovery` (crates/core-runtime, over the fault corpus)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
