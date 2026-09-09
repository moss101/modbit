# Task Card — M1.2 Implement SQLite migrations, projections, command idempotency

## Identity

- Task ID: M1.2
- Milestone: M1
- Canonical owner: core-runtime owner; crate `modbit-event-store` (doc 81 `event-state`)
- Requirement IDs: docs/31 (core tables `sessions/tasks/runs/turns/run_steps`, "Migration safety"), docs/30 ("Mutating commands are idempotent by `command_id`"), docs/33 "Idempotency", docs/13 ("Only Event Store append + transactional projection update can advance authoritative state"; "Projection consumers must be idempotent")
- Risk class: recovery-critical
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Versioned, checksummed migrations with a rollback plan each and refusal of newer schemas; projection rows for the five core aggregates written in the same transaction as the append and rebuildable from the log; a command ledger that makes a retried command with the same id replay its outcome instead of appending again.

## Non-goals

- Protocol-state, lease, checkpoint projections (M4); cloud Postgres mirror (M8).
- Command transport and authentication (M1.3).

## Required reading

`AGENTS.md`, `docs/13`, `docs/30`, `docs/31`, `docs/33`, `docs/19`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (M1.1 shipped schema v1 with a `schema_meta` version and newer-schema refusal; no ledger, no projections, no commands)
- production entry point: `EventStore::open` (migrations), `EventStore::append` (projections), `EventStore::execute_command` (idempotency)
- real effector/storage boundary: `core.db`
- tests found: M1.1 real-SQLite tests
- first missing/broken link: no `schema_migrations` ledger
- duplicate/drift risks: projection rows diverging from the log; prevented by deriving rows only through the domain reducers and `rebuild_projections`

## Invariants

- Migrations are append-only; an applied migration's SQL is checksummed and drift is an integrity error; a pre-ledger version-1 database is adopted without re-running; newer versions refuse write mode; unknown tables/columns are never dropped.
- Event insert and projection update commit in one transaction; an illegal transition anywhere in a batch rolls back the whole batch.
- Projections are a pure function of the log (`rebuild_projections` reproduces identical rows).
- Same `command_id` + same request hash → replay; same id + different hash → conflict; no duplicate events either way.
- Tasks reference existing sessions, runs existing tasks, turns existing runs, steps existing turns (foreign keys on).

## Implementation slice

- persistence/migrations: `schema::MIGRATIONS` (v1 events, v2 projections + commands + projection_state), `migrations::migrate` with `schema_migrations` ledger, `rollback_plans()`
- projections: `projections::{apply, rebuild, load_*}`; `EventStore::{task, session, run, turn, step, projection_offset, rebuild_projections}`
- idempotency: `commands` table, `CommandRecord`, `CommandOutcome::{Applied, Replayed}`, `Error::IdempotencyConflict`
- fixture: `tests/fixtures/db/core-v1-m1.1.db` produced by the M1.1 code via `examples/seed_fixture.rs`
- failure/recovery: `Error::Projection` with offset; `Error::SchemaTooNew`; `Error::Integrity` for checksum drift

## Verification

- integration (real SQLite): committed M1.1 fixture migrates 1→2, events untouched, projections derived, reopen is a no-op; checksum drift and newer ledger version refused; five aggregates projected after append, illegal batch rolled back atomically, rebuild identical; command replay/conflict/reopen
- existing M1.1 suite (raw log on an unprojected aggregate, hard kill, tamper) still passes
- E2E: hosted CI on three platforms

## Completion evidence

See `evidence.json`.
