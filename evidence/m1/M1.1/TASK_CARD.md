# Task Card — M1.1 Implement Session/Task/Run/Turn/RunStep domain and event store

## Identity

- Task ID: M1.1
- Milestone: M1 — Durable local shell and Core
- Canonical owner: core-runtime owner; crates `modbit-domain` (domain-events) and `modbit-event-store` (THE doc 81 `event-state` implementation)
- Requirement IDs: docs/13 aggregates, state machines, envelope, fencing; docs/31 `events`, local storage pragmas, migration safety; docs/30 event names; supports REQ-EV-0010 (offset resume) and REQ-EV-0101 (separate durable stores) whose own tasks follow
- Risk class: recovery-critical (canonical persistence)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Typed domain identities and the five core aggregates with their doc 13 state machines as explicit transition tables, plus an append-only SQLite event store with per-aggregate contiguous sequences, optimistic concurrency, a per-aggregate SHA-256 integrity chain, a content-addressed object directory for large payloads, and a store-wide offset cursor for session resume.

## Non-goals

- Migration framework, projection tables and command idempotency (M1.2).
- SurfaceProtocol transport (M1.3), UI (M1.4), snapshot/replay recovery orchestration (M1.5).
- Tool-call, approval, lease, checkpoint aggregates (their owning milestones); the envelope already names their aggregate types.

## Required reading

`AGENTS.md`, `docs/13`, `docs/19` (seven layers), `docs/30`, `docs/31`, `docs/33` (backpressure inline ceiling), `docs/81`.

## Existing-code audit

- classification: NOT-FOUND (both crates were empty M0.1 shells)
- production entry point: none yet (Core startup arrives with M1.3/M1.5); the store is consumed by M1.2 projections
- real effector/storage boundary: real SQLite file `core.db` (WAL, synchronous=FULL, foreign_keys=ON) and `objects/` directory
- tests found: none
- first missing/broken link: no domain types
- duplicate/drift risks: a second event log elsewhere; prevented by the canonical `event-state` claim (M0.4 lint)

## Invariants

- Only Event Store append advances authoritative state; illegal transitions are typed errors and leave projections untouched.
- `(aggregate_id, sequence)` unique; sequences contiguous from 1; `integrity_hash = sha256(prev || canonical content)`.
- Stale `expected_sequence` is rejected, never applied silently; stale kernel lease generation cannot resume a Run.
- `UnknownOutcome` only for effectful steps and never auto-retried.
- Payloads above 64 KiB are stored by SHA-256 reference and verified on read.
- A newer schema version refuses write mode.

## Implementation slice

- domain/API: `modbit-domain` ids (23), `Timestamp`, `StateMachine`, Session/Task/Run/Turn/RunStep aggregates with typed events and reducers, `EventEnvelope`, `PayloadRef`, `Actor`, `AggregateType`
- persistence/migrations: `modbit-event-store` schema v1 (`events`, `schema_meta`), `EventStore::{open, append, read_aggregate, read_session, head, last_offset, payload, verify_aggregate, integrity_check}`, `ObjectStore`
- failure/recovery: transactional append (IMMEDIATE), sequence conflict error, integrity error, schema-too-new error; hard-kill recovery proven
- evidence/observability: integrity chain verifiable offline

## Verification

- unit: 8 domain tests (ids, task, run fencing, turn repair loop, step unknown-outcome, verification stage), 1 store unit
- integration (real SQLite): append/read/chain/reopen; session offset resume across aggregates; tampered row and deleted row fail verification; newer schema refused
- fault: writer process hard-killed mid-append (SIGKILL); reopened store passes `PRAGMA integrity_check`, chain verifies, last row is a whole event, no committed event lost
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
