# Task Card — IMP-EV-0191 steer / collect / followup dispatch modes

## Identity

- Task ID: IMP-EV-0191
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-EV-0191 (ADOPT); owner label: Input Queue; subsystem: core-runtime
- Qualification: QUAL-EV-0191 — Concurrency test sends messages mid-run and verifies exact ordering/cancellation semantics.
- Evidence tier: real-system or production-equivalent

## Goal

Concurrency test sends messages mid-run and verifies exact ordering/cancellation semantics.

## Existing-code audit

- classification: IMPLEMENTED in this batch (inputs were applied only at boundaries, all as separate messages)
- production entry point: services/modbit-core/src/runtime.rs run_loop: SteeringPolicy at the turn boundary (STEER in order, COLLECT coalesced, FOLLOW_UP one per boundary with the rest carried) and the in-flight stream interruption (child cancellation token woken by the log offset watch; StepCancelled + TurnInterrupted)
- proof: COLLECT c1+c2 reach the model as one message, FOLLOW_UP f1 then f2 as separate turns, a STEER queued mid-stream cuts the stream and the next turn starts from it; TaskSteered order c1\nc2, f1, f2, s1

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0191_steering_policy_interrupts_replaces_coalesces_and_orders`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34437627213.json`, `ci-run-34437627213-tests.log`: hosted CI run 34437627213 on a1f7615, green on macOS, Linux and Windows
