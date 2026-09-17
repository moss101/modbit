# Task Card — M8.2 cloud session kernel lease + worker

## Identity

- Task ID: M8.2
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/24 "Cloud Core Worker", "Sync model"; docs/33 "Session kernel lease" (cloud half), "Cloud worker lifecycle"; docs/31 (`session_leases`, `commands`); REQ-EV-0109 (one canonical execution interface — the cloud session runs on the same `modbit-core` as the local profile, over the same log).
- Qualification: two real workers over a real Postgres 16, a real MinIO and the real `modbit-core` in the hosted `cloud` job — claim, materialize, run, relay, mirror, fence, resume.
- Evidence tier: real-system (containers for Postgres and MinIO; `modbit-core` spawned as a process per session; a scripted OpenAI-compatible provider on a real socket).

## Goal

A session in the cloud runs on a Cloud Core Worker: the worker claims the session's fenced lease, hosts the real Core for the session's tenant with the cloud log materialized into it, mirrors everything the Core journals back to the cloud log, executes what the API relays, renews the lease and is fenced when it loses it; a successor resumes from the same log at the next generation.

## Existing-code audit

- classification: MISSING before this task: `apps/cloud-worker` refused to run (M0 skeleton); the Core had no tenant flag and no way to import or export a mirrored log; the cloud store had no mirror, no command relay and no successor-safe claim; the API applied every command itself.
- production entry points: `apps/cloud-worker/src/{lib.rs,session.rs,core_process.rs,main.rs}` (`Config`, `start`, `Worker::stop`/`hosting`; the claim loop with capacity and the winding-down exclusion; `host` — heartbeat task from the claim, `host_inner` — provider into memory, import, local lease, mirror/relay/complete/trust/start loop, stop and fence; `CoreProcess::spawn` with `--tenant-id --tether-stdin`), `services/modbit-core/src/{main.rs,server.rs}` (`--tenant-id <uuid>` → `run_as`; `ImportMirroredEvents` and `ReadMirrorEvents` under `session.mirror`; `CloudWorker` capabilities including `repository.trust`; `TaskView.workspace_root`), `crates/event-store/src/store.rs` (`import_envelopes`: verbatim, chain-checked, forks refused, duplicates skipped), `crates/event-store/src/cloud/{mod.rs,schema.rs}` (`mirror` under the lease at its generation continuing each aggregate's chain; `claim_session`, `claim_ready_session(..., except)`, `renew_lease` refusing an expired lease, `held_by`; `enqueue_command`/`pending_commands`/`complete_command`; schema v2 `commands.session_id/body/completed_at_ms`), `apps/cloud-api/src/routes.rs` (`relay_if_held` → `202 PENDING` with `relayed_to`; `GET /v1/commands/{id}`; the task id is the creating command's id), `crates/protocol/proto/modbit/v1/surface.proto` (`MirroredEvent`, `ImportMirroredEvents`/`MirroredEventsImported`, `ReadMirrorEvents`/`MirrorEvents`, `TaskView.workspace_root`), `crates/policy/src/kernel.rs` (`cloud_isolated` profile with a ceiling; no host tool lists it — `TOOL_NOT_VISIBLE` until the sandbox arrives), `.github/workflows/ci.yml` (the `cloud` job builds `modbit-core` and runs the worker qualification).
- proof: `apps/cloud-worker/tests/cloud_worker.rs` — `qual_m8_2_a_worker_claims_the_lease_runs_the_task_relays_commands_mirrors_the_log_and_a_successor_resumes_fenced`: a task created through the API is named by its command; worker A claims at generation 1, imports `SessionCreated`/`TaskCreated`/`TaskQueued`, runs the task with the scripted provider to `READY_FOR_REVIEW`; the cloud log carries the run (`SessionLeaseAcquired`, `TaskStarted`, `RunCreated`, `PlanRecorded`, `TaskReadyForReview`) mirrored in order; `shell.exec` under `cloud_isolated` is `TOOL_NOT_VISIBLE`; a steer while held is `202 PENDING` relayed to A, completes `ACCEPTED` with `TaskInputQueued` on the cloud log by the time the outcome is readable; a pause is `REJECTED PAUSE_UNSUPPORTED`; a second task created while held is created by A under the command's id and runs; A's lease is expired under it and B claims at generation 2, B's Core materializes the whole log and adds exactly one event (its lease), every aggregate's sequence contiguous; A notices it is fenced (`Hosting::Fenced{1}`), stopping it releases nothing; B runs a third task; B's stop releases the session ready. `apps/cloud-api/tests/cloud_api.rs::qual_m8_1_leases_fence_by_generation_and_an_append_is_all_or_nothing` (extended): a cancel while held is relayed and replays; the owner's completion is readable; a fenced owner's mirror is `StaleLease`; a hash that does not continue the chain is `Integrity`. `crates/event-store` unit: `import_envelopes` round trip.

## Limitations

- The worker's Core hosts the workspace on the worker's own host; the sandboxed workspace, guest and Sandbox Gateway calls arrive with M8.3–M8.5 (until then `cloud_isolated` compiles no host tool into a task's surface).
- Pause/resume are refused `PAUSE_UNSUPPORTED` (the Core has no pause; cancel and steer are the controls).
- A worker's heartbeat metrics are not yet exported; the lease row (`heartbeat_at_ms`) is the record.

## Verification

Named tests (the hosted `cloud` job; elsewhere they say `SKIPPED` and pass):

- `qual_m8_2_a_worker_claims_the_lease_runs_the_task_relays_commands_mirrors_the_log_and_a_successor_resumes_fenced`
- `qual_m8_1_leases_fence_by_generation_and_an_append_is_all_or_nothing` (relay and mirror assertions)
- `qual_m8_1_commands_are_idempotent_events_replay_by_cursor_and_tenants_never_cross` (control requests)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
