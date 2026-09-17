# Task Card — M8.7 local→cloud checkpoint handoff

## Identity

- Task ID: M8.7
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "Handoff local → cloud" (immutable workspace checkpoint, Git metadata, task/protocol state, tool capability requirements, opaque secret handle references; never raw secret values; capability parity verified before the execution owner switches); docs/24 "Sync model" (remote coding operates on explicit Git/checkpoint handoff bundles; one log); docs/30 (`POST /v1/handoffs`); REQ-EV-0063 (EnvironmentHandoffBundle); docs/02 secrets custody (a provider key or forge token is never written to any file).
- Qualification: a real local Core runs a task's first turns; the export parks it and writes the bundle; a real Cloud API (Postgres + MinIO) refuses a bundle whose capabilities the cloud does not serve, admits the real one and replays the admission; a real worker materializes the continuation and runs it inside a sandbox from the gateway (Firecracker MicroVM on CI, the reference backend elsewhere) to completion.
- Evidence tier: real-system.

## Goal

A task started on the laptop continues in the cloud from its checkpoint: the same log, the same worktree (committed history and the dirty edit), the same transcript, under `cloud_isolated` inside a sandbox — and nothing secret leaves the laptop.

## Existing-code audit

- classification: MISSING before this task: `StartCloudHandoff` was a name in docs/30 and `POST /v1/handoffs` a row; no bundle, no admission, no materialization existed.
- production entry points: `services/modbit-core/src/handoff.rs` (`export`: park at the boundary, checkpoint, `events.jsonl`, `objects/`, `repo.bundle`, `manifest.json`, `TaskHandedOff`), `services/modbit-core/src/server.rs` (`ExportHandoff`; `RebindTaskWorkspace` retiring the old profile's leases; `ImportObjects`; `ImportMirroredEvents.admitted_handoff`; `StartTask` granting the profile's default lease when none is valid under it and provisioning the sandbox for `cloud_isolated`), `crates/domain/src/task.rs` (`TaskHandedOff`, `TaskHandoffAdmitted`, `TaskWorkspaceRebound {workspace_root, reason, execution_profile}`), `crates/protocol/proto/modbit/v1/surface.proto` (the commands and results), `crates/event-store/src/cloud/mod.rs` (`import_handoff`: verbatim envelopes under the cloud tenant, chain-checked, tenant-scoped rows), `apps/cloud-api/src/routes.rs` (`PUT /v1/objects`; `POST /v1/handoffs`: replay, capability parity against `CLOUD_CAPABILITIES`, the parts, the import, the admitted manifest, `TaskHandoffAdmitted`, ready), `apps/cloud-worker/src/handoff.rs` (`pending`, `materialize`: clone the bundle, check the branch out, lay the checkpoint over it, import the objects, rebind to `cloud_isolated`, trust), `apps/cloud-worker/src/session.rs` (materialize before starting; a parked task a handoff readied is started).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_7_a_local_task_hands_off_to_the_cloud_and_continues_from_its_checkpoint_in_a_sandbox` — the laptop's Core (a real `modbit-core` process, CLI client, a provider key and a forge token configured) runs `plan.update` and `change.apply`; `ExportHandoff` parks the run (`Waiting`), the manifest names the task, the checkpoint, the git head, the capabilities, `forge-token` as a handle; every bundle file is scanned for both secret values (none); the parts and objects are uploaded through `PUT /v1/objects`; a manifest claiming `browser.control` is `409 CAPABILITY_PARITY`; the real one is `201` and its retry `200` with the same `bundle_hash`; the cloud log then carries `TaskHandoffAdmitted`, `CheckpointCommitted`, `RunSuspended`; a worker with a sandbox gateway materializes and continues: `TaskWorkspaceRebound`, `SandboxLeaseAcquired`, `TaskResumed`, `RunResumed`, `TaskReadyForReview`; the cloud model saw the laptop's turns (`plan version 1 recorded`); `fs.read summary.md` inside the sandbox returns the laptop's edit; `shell.exec git log/status` inside the sandbox shows the `base` commit and the uncommitted edit.

## Limitations

- The bundle is a directory the client uploads part by part; a single-request streaming upload and resumable transfer are not built.
- The continuation always runs under `cloud_isolated`; a handoff into another cloud profile is not exposed.
- The cloud's results are not handed back to the laptop (docs/24: nothing is synced back); the desktop follows the cloud log.
- The parity check is over the tools the run used and the cloud's fixed capability set; per-tenant capability sets are not configurable.

## Verification

- `qual_m8_7_a_local_task_hands_off_to_the_cloud_and_continues_from_its_checkpoint_in_a_sandbox`
- Domain: `TaskWorkspaceRebound` moves the root and the profile; `SandboxReleased`/`SandboxLost` after a terminal state (M8.5)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
