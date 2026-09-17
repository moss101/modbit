# Task Card — IMP-EV-0063 EnvironmentHandoffBundle

## Identity

- Task ID: IMP-EV-0063
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0063 (transfer task/plan/context/evidence/git delta/runtime requirements/secret refs after capability negotiation); QUAL-EV-0063 (local→cloud handoff preserves state but never embeds secret values); docs/21 "Handoff local → cloud".
- Qualification: the M8.7 end-to-end scenario on real components (local Core, Cloud API over Postgres + MinIO, worker, sandbox gateway).
- Evidence tier: real-system.

## Goal

The handoff bundle is the environment's transfer: the task's whole log (task, plan, context, evidence — every object the log references), the git delta (`repo.bundle`, all refs) with the checkpointed worktree over it, the runtime requirements (the capabilities the run's tools required) and secret references as handles — negotiated by the cloud's capability parity check before the execution owner switches — and never a secret value.

## Existing-code audit

- classification: MISSING before M8.7.
- production entry points: `services/modbit-core/src/handoff.rs` (the bundle; `secret_handles` from the lease's `secret.use` against the custody this Core holds, as handles), `apps/cloud-api/src/routes.rs` (`CAPABILITY_PARITY`), `apps/cloud-worker/src/handoff.rs` (materialization).
- proof: in `qual_m8_7_…`: the laptop's Core holds a provider key and a forge token; every file of the bundle (`events.jsonl`, `manifest.json`, `objects/*`, `repo.bundle`) is scanned for both values and carries neither; the manifest's `secret_handles` names `forge-token`; the bundle's log carries the checkpoint and the park; the cloud continuation sees the laptop's plan, transcript, edit and history; a bundle naming `browser.control` is refused before anything switches.

## Limitations

- As M8.7's: part-by-part upload; the cloud's capability set is fixed; no return path.

## Verification

- `qual_m8_7_a_local_task_hands_off_to_the_cloud_and_continues_from_its_checkpoint_in_a_sandbox`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35268427152.json` — hosted run 35268427152 at cd2d6aa on main (attempt 2: a macOS desktop E2E approval-state flake re-run), green on macOS, Linux and Windows; the cloud job on a real Firecracker MicroVM
