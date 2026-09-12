# Task Card — M6.2 capacity ticket allocator

## Identity

- Task ID: M6.2
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: docs/14 "Capacity tickets", docs/33 "Scheduler" (capacity tickets); REQ-EV-0272 via IMP-EV-0272; owner label: core-runtime; subsystem: core-runtime (`crates/core-runtime::capacity`, `services/modbit-core/src/capacity.rs`)
- Qualification: M6.2 — a typed resource vector with leased, generation-fenced tickets that admission consumes before anything starts; QUAL-EV-0272 — capacity exhaustion denies launch without partial side effects.
- Evidence tier: real-system (the real Core with a one-slot pool, two tasks against a scripted OpenAI-compatible server that holds a request, a one-second lease that lapses)

## Goal

Capacity as a typed resource vector — model concurrency, terminal slots, sandbox slots, browser slots, memory budget, provider quota — not an agent count; tickets with lease expiry and generation fencing, consumed before spawn/sandbox.

## Existing-code audit

- classification: MISSING before: nothing bounded how many runs a Core executed at once; `Waiting(Capacity)` existed in the task state machine with no producer.
- production entry points: `crates/core-runtime/src/capacity.rs` (`ResourceVector` with `parse`, `shortfall`, `one_run`; `CapacityPool::allocate` all-or-nothing, `renew` fenced by generation, `expire`, `release`; `CapacityRefused` typed); `services/modbit-core/src/capacity.rs` (the pool from `MODBIT_CAPACITY` / `MODBIT_CAPACITY_TTL_MS`, `acquire` recording `CapacityTicketGranted` / `CapacityDenied` and lapses, `renew`, `release`, `view`); `services/modbit-core/src/runtime.rs` (`take_run_ticket` before a run is created or resumed; renewal at every turn boundary with re-acquire or `LoopEnd::CapacityLost` → `Waiting(Capacity)` + `CAPACITY_LOST`; release at the loop's end); `crates/core-runtime/src/diagnostics.rs` (`CAPACITY_LOST`, corpus-pinned); `services/modbit-core/src/server.rs` (`GetCapacity`); `apps/cli` (`capacity show`).
- proof: with `MODBIT_CAPACITY=model=1`, task A holds the slot and task B's `StartTask` is refused `CAPACITY_EXHAUSTED` naming `model_concurrency` with `CapacityDenied` on B's log and no `TaskStarted`, `RunCreated`, `AgentNodeCreated` or lease behind it; A finishes, its ticket is `RELEASED`, B starts and finishes; with a one-second lease, a run held by its provider for four seconds lapses (`LAPSED` on the log), the waiting task takes the freed slot and finishes, and the lapsed run either re-takes a slot at its next boundary or suspends `Waiting(Capacity)` with `CAPACITY_LOST` (retryable) and finishes once resumed; the pool ends empty.

## Limitations

- Terminal, sandbox and browser dimensions are declared and enforced by the same allocator but no owner consumes them yet: the run-level ticket is the only consumer until subagent admission (M6.3) and the sandbox/terminal owners take theirs.
- The pool is process-local: a Core restart starts with an empty pool (every run of the old process is suspended, and a resume takes a ticket again).
- Lapse is enforced at turn boundaries; a run inside a long provider call keeps running until the boundary, where it either re-takes a slot or suspends.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_2_capacity_tickets_gate_runs_all_or_nothing_and_lapse_at_expiry`
- `a_ticket_is_all_or_nothing_and_exhaustion_names_the_dimension`
- `tickets_lapse_at_expiry_and_a_stale_generation_cannot_renew`
- `the_vector_parses_and_refuses_what_it_cannot_read`
- `qual_ev_0245_fault_corpus_produces_stable_diagnostic_features` (`crates/core-runtime/tests/diagnostics_corpus.rs`, the `CAPACITY_LOST` pin)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
