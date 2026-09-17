# Task Card — M8.5 typed guest process/fs/PTY RPC

## Identity

- Task ID: M8.5
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "`modbit-guest`" (process start/wait/cancel, PTY attach/replay, file operations within the mounted workspace), "Execution profiles" (`cloud_isolated`), "Sandbox substrate boundary" (typed guest RPC with task/turn/call/effect ids); docs/24 "Cloud Core Worker" (executes the Agent Runtime and calls the Sandbox Gateway); docs/30 "Version compatibility"; REQ-EV-0289 (typed capability/effect-bound guest RPC; an unknown or stale version or capability is rejected), REQ-EV-0290 (mount-level pinning of protected paths against processes), REQ-EV-0109 (one canonical execution interface — the same tool contracts, a cloud adapter).
- Qualification: the extended conformance suite on a real Firecracker MicroVM and on the reference backend on every OS; the cloud worker end to end — a `cloud_isolated` task whose `shell.exec`, `fs.read`, `fs.list` and `fs.stat` act inside its sandbox (under the MicroVM's kernel in the hosted job), the sandbox on the cloud log and released when the task ends.
- Evidence tier: real-system.

## Goal

Processes, PTYs and directory operations as typed, capability-bound guest calls with replay across a lost link; the Core's tools acting inside a task's sandbox under `cloud_isolated`.

## Existing-code audit

- classification: PARTIAL before this task: the guest served one-shot `exec` and file reads/writes (M8.3); the Core's tools had no sandbox path and `cloud_isolated` compiled no host tool.
- production entry points: `crates/protocol/proto/modbit/v1/guest.proto` (protocol 1.1: proc start/follow/write/cancel, PTY resize, list/stat/mkdir/remove/rename), `services/modbit-guest/src/procs.rs` (the process table: bounded rings with cursors, pipes and PTYs, timeouts, cancel), `services/modbit-guest/src/serve.rs` (dispatch with capability binding; mount-level pinning of protected paths on a MicroVM), `crates/sandbox/src/link.rs` (the typed calls), `crates/sandbox/src/backend/{mod.rs,reference.rs,microvm.rs}` (`reconnect`), `crates/sandbox/src/port.rs` (`SandboxPort`, `SandboxIdentity`), `crates/sandbox/src/client.rs` (`GatewayClient`, `SandboxHandle`; feature `client`), `apps/sandbox-gateway/src/routes.rs` (the call kinds, `relink`), `crates/tools/src/direct.rs` (the sandbox path of `shell.exec`, `test.run`, `fs.read`, `fs.list`, `fs.stat`; `SANDBOXED_PROFILES`), `crates/tools/src/pipeline.rs` (`InvokeContext.sandbox`), `crates/policy/src/kernel.rs` (the `cloud_isolated` default lease), `crates/domain/src/task.rs` (`SandboxLeaseAcquired`, `SandboxReleased`, `SandboxLost`), `services/modbit-core/src/{sandboxes.rs,tools.rs,server.rs,runtime.rs}` (`ConfigureSandboxGateway`; provisioning at `StartTask`, the default lease for a task materialized from the cloud log, release when the task ends), `apps/cloud-worker/src/{lib.rs,session.rs}` (the gateway configuration handed to each Core).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_5_a_cloud_isolated_tasks_tools_act_inside_its_sandbox_and_the_sandbox_is_released_when_the_task_ends` (the task's `shell.exec` runs in the guest — under `6.1.155` on the MicroVM — and writes a file; `fs.read` reads it back; `fs.list` and `fs.stat` see the guest's workspace; a path outside the workspace is refused; a process cannot write a protected path on the MicroVM; `SandboxLeaseAcquired` names the backend, the isolation and the verified image; a cancel through the API ends the task and `SandboxReleased` follows; the gateway records the sandbox `DESTROYED`), `qual_m8_2_…` (now every task of the lease/mirror scenario runs in a sandbox: a process sees nothing of the worker's environment); `apps/sandbox-gateway/tests/gateway.rs` conformance steps `proc_start`, `proc_follow_first`, `relinked`, `proc_replay_across_links`, `proc_replay_from_start`, `proc_stdin`, `proc_cancel`, `pty`, `fs_dir_ops`, `fs_dir_protected_refused`, `fs_workspace_root_not_removable`, `fs_dir_outside_refused`, `proc_protected_path_enforced` (asserted of the MicroVM) on both backends.

## Limitations

- The change engine (`change.apply`), git worktrees, search and verification runs are not routed through the sandbox yet; the sandboxed workspace becomes the candidate the Core verifies and ledgers with the handoff (M8.7).
- A Core that dies leaves its sandboxes to the gateway until the worker sweeps them; sandbox loss and recovery are M8.9.
- Followed processes are relayed one call at a time through the gateway; a streaming transport for PTY output to a viewer is M8.8's remote stream.

## Verification

- `qual_m8_5_a_cloud_isolated_tasks_tools_act_inside_its_sandbox_and_the_sandbox_is_released_when_the_task_ends`
- `qual_m8_2_a_worker_claims_the_lease_runs_the_task_relays_commands_mirrors_the_log_and_a_successor_resumes_fenced`
- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract`, `qual_m8_3_the_reference_backend_passes_the_backend_contract` (the M8.5 steps inside)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35249710305.json` (hosted run 35249710305 at c63e3ce, all 14 jobs green)
