# Task Card — M9.5 emergency stop

## Identity

- Task ID: M9.5 (milestone task, docs/43 "M9 — Engineering memory/effects/security hardening")
- Milestone: M9; release: RELEASE_ZERO
- Specification: `docs/23_SECURITY_POLICY_EFFECT_LEDGER.md` "Emergency stop": global stop revokes active capability leases, blocks new effects, cancels safe tool calls, freezes dangerous external operations at broker/gateway where possible and marks ambiguous outcomes for reconciliation. Related: REQ-EV-0085 (host-owned watchdog / emergency stop, IMP-EV-0085 COMPLETE in M7.6), docs/14 "Compound execution and interruption", docs/21 "resource quotas and emergency stop".
- Evidence tier: release-critical (execution, recovery, a security boundary)

## Goal

Make the session-wide stop what docs/23 says it is: not only a refusal of new effects, but a cancellation of what is in flight with honest outcomes on the log, lasting for the session and across a Core restart.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. Traced from `EmergencyStop` in `services/modbit-core/src/server.rs` to its effects:
  - PRODUCTION-WORKING: `EmergencyStopActivated` journaled on the session (a projection, so it outlives the Core); every active capability lease revoked with `EMERGENCY_STOP: <reason>`; the Capability Kernel (`crates/policy/src/kernel.rs`, decide step 1) refuses every non-read-only effect of a stopped session with `EMERGENCY_STOP` — proven by `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals`; the browser's host-owned stop halts input independently of the model loop — `qual_ev_0085_an_emergency_stop_halts_browser_input_before_the_host_with_the_reason_on_record` (M7.6).
  - the first missing links: (1) a running loop fences on the session lease generation (`lease_lost`), which the stop does not change, so after a stop the loop kept turning: every effect refused, model calls still spent, until no-progress escalation; (2) a process a tool was running (`shell.exec`, `test.run`, an engine stage) ran to its exit or timeout: neither the direct runner in `crates/tools/src/direct.rs` nor the engine's `BrokerRunner` in `services/modbit-core/src/verify.rs` had a cancellation path to the broker; (3) a cancelled process was recorded as an `ApplicationFailure` (and a cancelled `test.run` as a successful call carrying a `CANCELLED` report), not as a cancelled call; (4) `StartTask` would start a new run in a stopped session.
- production entry points now:
  - `services/modbit-core/src/server.rs` `EmergencyStop`: after journaling and revoking, cancels every live loop of the session (`Runtime::cancel`); `StartTask` refuses a stopped session with `EMERGENCY_STOP`.
  - `crates/tools/src/pipeline.rs` `InvokeContext.cancel` and `crates/tools/src/direct.rs::run_process`: the run's token sends the broker's `Cancel` for the process's session (also when the cancel arrives before `Started`); the outcome maps to `ToolStatus::Cancelled`; a cancelled `test.run` is a cancelled call.
  - `services/modbit-core/src/verify.rs` `BrokerRunner.cancel` and `Runtime::cancel_token`: an engine stage running when the run is cancelled is cancelled at the broker and reported cancelled.
- proof: the qualification below on the real Core with the real terminal broker.

## Verification

- `m9_5_emergency_stop_cancels_the_check_in_flight_ends_the_run_and_outlives_a_restart` (`services/modbit-core/tests/surface_protocol.rs`): a scripted agent plans and runs a check that would take 600 s; the stop lands while the process is running (found by `pgrep`); the process is gone within 15 s; the task ends `Cancelled` with `ToolCallCancelled`, `CapabilityLeaseRevoked` naming `EMERGENCY_STOP`, `TurnInterrupted`, `RunCancelled`, `TaskCancelled` on its log and `EmergencyStopActivated` on the session; the model server received no request after the stop; the write the script had queued never happened; `StartTask` for a fresh task in the session is refused `EMERGENCY_STOP`; after `kill` and restart of the Core it still is.
- unchanged and green with the change: `m2_5_…`, `qual_ev_0085_…`, the cancellation and kill-point suites (`m4_6_…`), `residue_of_a_check_the_agent_runs_is_recorded_not_attributed`, `m2_7`, `m2_8`, `qual_px_039`; `modbit-tools` 17/17.

## Failure and negative proof

- The negative proof is the same test against the code before this task: the process outlived the stop (the test's "the check kept running after the stop" assertion fired on the first run of this task, before `BrokerRunner`/`run_process` could cancel) and the call was recorded a success carrying a `CANCELLED` report.
- Faults kept from M4.6: a cancellation landing after an effect was sent is `ToolCallUnknownOutcome` for reconciliation (`ReconcileToolCall`), never replayed.

## Limitations

- A stop is per session and lasts for the session; there is no command to lift it (docs/23 names none). Work continues in a new session.
- Dangerous external operations already accepted by a remote (a forge call in flight, an MCP effect) cannot be recalled; they are recorded as unknown outcomes for reconciliation, as docs/23 allows ("where possible").
- Cloud-isolated tasks: the loop cancellation applies; the sandbox's process cancellation follows the guest protocol (M8.5) and is not separately measured here.

## Evidence

- `evidence.json` in this directory; PR #21 squashed to `main` as `d544115`; hosted CI run 35565188980 green on macOS, Linux and Windows (`ci-run-35565188980.json`; the macOS desktop-E2E job on rerun after the documented timing flake)
- docs/23 "Emergency stop" carries the as-built paragraph
