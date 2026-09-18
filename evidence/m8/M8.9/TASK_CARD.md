# Task Card — M8.9 sandbox-loss recovery

## Identity

- Task ID: M8.9
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "Sandbox recovery" (in-flight tool calls marked unknown, protected effects reconciled, a fresh sandbox provisioned, the latest valid checkpoint restored, the run resumed; external side effects never replayed automatically); docs/19 "Resume algorithm" (unknown outcomes reconciled against the effect ledger; sandbox resources reattached by lease); docs/51 E2E-018 (kill the cloud MicroVM mid-task after a checkpoint: lease loss detected, unknown outcomes reconciled, fresh sandbox restored from the checkpoint, the task resumes without a duplicated external effect); docs/24 (one log).
- Qualification: the cloud worker end to end on a real sandbox (a Firecracker MicroVM on CI, the reference backend elsewhere): the sandbox's process killed after a checkpointed turn.
- Evidence tier: real-system.

## Goal

A cloud task survives the loss of its sandbox: the call in flight keeps its unknown outcome, the loss is on the log, a fresh sandbox takes the lost one's place with the latest checkpoint's worktree written into it, and the run continues there.

## Existing-code audit

- classification: MISSING before this task: `SandboxLost` existed as an event no one journaled; a cloud task's checkpoints captured the host seed, not the guest's worktree; a sandbox that stopped answering left every later call failing.
- production entry points: `services/modbit-guest/src/serve.rs` (`fs.snapshot`: the workspace hashed, `.git` and symlinks skipped, bounded), `crates/protocol/proto/modbit/v1/guest.proto`, `crates/sandbox/src/{link.rs,port.rs,client.rs}` (`fs_snapshot`, `remove`, binary-safe `write_file`), `apps/sandbox-gateway/src/routes.rs` (`fs.snapshot`; `content_base64`; a relink that fails records `LOST`, tears the remains down and answers `410 SANDBOX_GONE`), `services/modbit-core/src/checkpoint.rs` (`dirty_state_sandbox`: the guest's worktree against the seed), `services/modbit-core/src/runtime.rs` (a turn-boundary checkpoint for a cloud task that ran a sandboxed tool; the `SANDBOX_UNAVAILABLE` hook before the model hears the result), `services/modbit-core/src/sandboxes.rs` (`recover_if_lost`: relink once, `SandboxLost`, a fresh sandbox, the checkpoint written in, the browser host re-attached, `SandboxRestored`, the note to the model), `crates/domain/src/task.rs` (`SandboxRestored`).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_9_a_lost_sandbox_is_replaced_and_restored_from_the_latest_checkpoint_and_the_task_resumes` — a turn writes `work.txt` and appends to `NOTES.md` in the guest; `CheckpointCommitted` follows the turn; the sandbox's process (from the gateway's record) is killed; the model's next call answers `UNKNOWN_OUTCOME` / `SANDBOX_UNAVAILABLE` with the recovery note naming the lost sandbox, the fresh one and the checkpoint restored; the log carries one `ToolCallUnknownOutcome`, one `SandboxLost` (the first sandbox), two `SandboxLeaseAcquired` (different ids), one `SandboxRestored` (the second sandbox, replacing the first, the checkpoint's id, two files written); the gateway's record of the first sandbox is `LOST`; the model's retry runs in the second sandbox and reads `v1` and `more` — the restored worktree; the task reaches review.

## Limitations

- The loss is detected at the next sandboxed call (no liveness poll while the run waits on the model).
- A checkpoint of a worktree beyond 20 000 files or 256 MiB of changes is refused; the seed is the worker's host copy, so a cloud task's checkpoint is relative to that seed.
- Processes that were running in the lost sandbox (followed processes, PTYs) are gone; their handles answer as lost.
- The person is not asked before the replacement: recovery is automatic, the effects after the checkpoint are the model's to redo, a protected effect of unknown outcome stays for reconciliation as before (docs/19).

## Verification

- `qual_m8_9_a_lost_sandbox_is_replaced_and_restored_from_the_latest_checkpoint_and_the_task_resumes`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35290929921.json` — hosted run 35290929921 at c20494f on main, green on macOS, Linux and Windows; the cloud job on a real Firecracker MicroVM with Chrome for Testing's headless shell (`HeadlessChrome/153.0.8010.47`) answering CDP inside the guest
