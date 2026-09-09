# Task Card — M1.5 Crash/restart snapshot + event replay

## Identity

- Task ID: M1.5
- Milestone: M1
- Canonical owner: core-runtime (`modbit-core` startup, `modbit-event-store` recovery); desktop owner for the recovery banner
- Requirement IDs: docs/19 "Resume algorithm" steps 1–3 as applicable to M1 (lease generation base, load projection + event tail and validate hashes, reconstruct state), docs/33 startup steps 2 and 11 (integrity check; `CoreReady` only after recovery), docs/54 faults 1–2 (Core killed before commit / after commit before response), docs/39 PX-023 recovery state ("show what the Core recovered"), docs/43 M1 milestone proof. Substrate for REQ-EV-0101 / REQ-EV-0121 whose own tasks follow.
- Risk class: recovery-critical
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

On every start the Core runs an explicit recovery pass before it announces readiness: SQLite integrity check, hash-chain verification of every aggregate, projection catch-up from the log, and a persisted monotonically increasing boot generation. Clients can ask what was recovered, and the desktop shows it. A kill-point suite proves that hard-killing the Core at arbitrary points in a command stream never duplicates or tears state.

## Non-goals

- Per-session kernel leases and fencing across desktop/cloud/CLI (IMP-EV-0054/0273), protocol-state, checkpoint and compaction recovery (M4), UnknownOutcome reconciliation (M2/M4), terminal/browser reattach (M2/M7).

## Required reading

`AGENTS.md`, `docs/19`, `docs/31`, `docs/33`, `docs/39`, `docs/54`, `docs/93`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (store durability + hard-kill recovery from M1.1/M1.2; Core reopened the store but ran no explicit recovery; desktop resubscribed from its cursor after restart but could not say what was recovered)
- production entry point: `server::run` → `EventStore::recover_on_start` before binding the endpoint; `GetRecoveryReport` command; desktop main fetches it on every (re)connect
- real effector/storage boundary: `core.db`
- first missing/broken link: no recovery pass, no report

## Invariants

- The endpoint is bound and the ready line printed only after recovery succeeds; a tampered log fails recovery and the Core does not start.
- Boot generation strictly increases per start and is persisted with the store.
- Projection rows always equal the fold of the log (rebuilt when the cursor lags).
- For any kill point: acknowledged commands are never lost; committed-but-unacknowledged commands replay; never-committed ones are accepted at most once; no duplicate task for one `command_id`.

## Implementation slice

- domain/API: `GetRecoveryReport` / `RecoveryReport` (surface.proto, additive; TS regenerated)
- service/effector: `EventStore::recover_on_start` (+ `RecoveryOutcome`); Core startup ordering; `core:recovery` IPC → renderer recovery banner text
- failure/recovery: kill-point suite; tamper refusal; lagging cursor rebuild

## Verification

- store: recovery verifies chains, bumps boot generation, rebuilds a deliberately lagged projection, refuses a forged event
- Core (real binary): kill-point suite — a writer streams CreateTask while the Core is SIGKILLed at random moments across five rounds (hundreds of tasks, most rounds with commands in flight); after each restart the report is served, replaying every command of the round yields the exact expected task set with no duplicates
- desktop E2E: recovery banner reports "recovered 1 session(s) and 1 task(s)" and "nothing left to reconcile" after a Core SIGKILL
- E2E on hosted CI: three platforms

## Completion evidence

See `evidence.json`.
