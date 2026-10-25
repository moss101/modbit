# Task Card — PX-128 Cloud handoff, watch, approve and return (audit record; no code change in this pass)

## Identity

- Task ID: PX-128 (REQ-PX-128; owner desktop); qualification QUAL-PX-128 / PX-E2E-128
- Evidence tier: release-critical (session lease, policy, data-egress boundary)
- Status in the graph: NOT_STARTED (prerequisites PX-042 NOT_STARTED, PX-045 COMPLETE, M8.1/M8.7 COMPLETE, DOC-PX-013 COMPLETE)

## Existing-code audit (2026-10-25)

- CLI `login`, `logout`, `status`, `handoff` (two-step, confirmation lists what leaves, Core-side policy and secret scan), `list`, `watch` (cursor resume), `approvals`, `approve` (second answer refused `APPROVAL_NOT_OPEN`), `policy`: PRODUCTION-WORKING against the reference backend (`apps/cli/src/cloud.rs`, commit 23d1357a). Baseline re-run in this pass, with a real Postgres and the real Cloud API, worker and reference sandbox gateway: `apps/cloud-worker/tests/px_128_cloud_client.rs` 1 passed, exit 0 (`baseline-px_128_cloud_client-run.txt`).
- `cloud return`: NOT-FOUND. No CLI verb, no API route, no Core command, no event. `docs/24` states it plainly: "nothing is synced back — the continuation's results are on the cloud log and in the cloud workspace".
- Desktop Continue in Cloud action and cloud-sessions list: NOT-FOUND (no cloud code under `apps/desktop`).
- The desktop answering an approval raised in the cloud: NOT-FOUND (no desktop cloud client).

## First missing links (in order)

1. The delta does not leave the cloud. The worker mirrors only events to the tenant store (`store.mirror`); the objects a cloud `CheckpointCommitted` names (manifest and changed-file blobs, taken in the sandbox relative to the seed) live in the worker Core's object store and are never put in the tenant object store, so `GET /v1/outputs/{hash}` cannot serve them. Needed: the worker uploads the objects of each checkpoint it mirrors (a Core read-object command for the worker, `CloudStore::put_object`).
2. A way to apply a returned delta locally through the Core. The CLI must not write the workspace itself (path policy, Kernel, provenance): needs a local Core command (for example `ImportReturn {task_id, bundle_dir}`: verify every hash and path, check the base against the handed-off checkpoint, apply through the change engine, journal a `TaskReturnedFromCloud {bundle_hash, checkpoint_id, files}` event, un-park the run), with the proto message, the TS generation (`packages/surface-protocol`), the domain event and its projection.
3. The API return route (admit a return only for the task's own tenant and session, bounded, hashed) and the CLI `cloud return` verb that downloads, verifies and calls the Core.
4. The desktop: cloud-sessions pane, Continue in Cloud with the same two-step confirmation, an execution-location pill, approval answered from the desktop under the same session-lease fence.

None of 1-4 is a local-only change: 1-3 add a protocol message, a domain event and a worker data flow (a schema and persistence change), and 4 adds a renderer and IPC surface (a preload and channel-inventory change).

## External blockers (not fakeable)

- A real staging Cloud API with a real cloud worker and sandbox (the MicroVM isolation is the CI job's; the reference backend proves the protocol, not the isolation), the packaged desktop, and the owner's identity/credentials for it. The "lost network during watch" and "second answer from the other client" clauses can be proven locally against the reference backend (the CLI half is), the packaged-desktop half cannot.

## Verification in this pass

- Baseline test above only. No code was changed for PX-128, no status was advanced. Graph refusal: `cannot AUDITING PX-128: prerequisite PX-042 is NOT_STARTED`.
