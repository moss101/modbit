# Task Card — IMP-EV-0242 Durable facts vs live interception separation

## Identity

- Task ID: IMP-EV-0242
- Milestone: M4 Durable recovery spine (P0) (DR-M0-005: after the durable store and the kill-point recovery suite)
- Requirement: REQ-EV-0242; owner label: Core Architecture; subsystem: governance
- Qualification: `QUAL-EV-0242` — Restart loses no durable truth even though hook process resets.
- Evidence tier: real-system (the real Core hard-killed and restarted twice; the store read directly while no Core runs)

## Goal

Durable state remains in stores; live control is ephemeral. A Core process can die at any moment and everything that was durable reads back identical, while everything that was live control is gone with the process and comes back only from whoever owns it.

## Existing-code audit

- classification: IMPLEMENTED by design, proven here. The separation is docs/33's: the event log, the object store and the projections rebuilt from them are the durable truth; the running loop, cancellation, steering queues, the terminal broker connection and a provider registration handed over the socket (`ConfigureProvider`, never journaled because the request carries a credential) are process-local control. The desktop already re-hands its keychain-held credential to every restarted Core (`handProviderToCore`); the CLI joins the session lease afresh.
- production entry points:
  - `crates/event-store` — `read_session` (every event at its offset with its integrity hash), `verify_aggregate` (hash chain per aggregate), the object store's digest-checked `get`; `recover_on_start` verifies every aggregate before the endpoint binds.
  - `services/modbit-core/src/server.rs` — `ConfigureProvider` (in memory only); `StartTask` now refuses a *named* endpoint that is not registered in this Core (`NO_PROVIDER`, naming the endpoint) instead of starting a run that would fail at its first model call: a registration is live control and its absence after a restart is reported at the boundary, never guessed.
  - `services/modbit-core/src/runtime.rs` `reconcile_after_restart` — the only events a restart may add belong to tasks that were live (M4.1); a task whose run had ended gets nothing.
- proof: a real coding task runs to `ReadyForReview` under a provider registered live with a credential over the socket, then a `DELTA` checkpoint is taken; the client's views (`TaskStatus`, `ProtocolStateView` and its digest, `CheckpointList`, `SessionSnapshot`, `ReviewBundle`) and the wire replay from offset zero are captured; the Core is hard-killed; with no Core running the store is read directly: every event with offset, aggregate, sequence, hash and payload, every aggregate chain verified, every object re-hashed, and the credential found in no file under the profile. The restarted Core refuses to start a new task on the endpoint (`NO_PROVIDER`: the registration was live control); every view reads back equal, the protocol-state digest equal, the wire replay identical offset for offset, and the only new events are the fresh task's own creation; the client re-registers the provider and the fresh task runs to `ReadyForReview` against the same durable session; after that Core is killed too the first life's events are unchanged byte for byte, every object still present, and the credential still nowhere.

## Limitations

- The typed Hook Bus (REQ-EV-0042/0139/0240, M9) is the "hook process" of the qualification's wording; it does not exist yet. What resets here is the live control the Core holds today (provider registration, running loop, broker connection, steering). When the bus lands, its handler registrations join this test as one more ephemeral control that a restart must lose and a client must re-register.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0242_restart_loses_no_durable_truth_while_live_control_resets`
- Regression: the M4.6 kill-point suite (`qual_m4_6_kill_point_suite_every_recovery_boundary_survives_a_process_abort`) covers a task that *is* live at the kill.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
