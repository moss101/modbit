# Task Card — IMP-EV-0109 Separate local and cloud execution contracts

## Identity

- Task ID: IMP-EV-0109
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0109 (one canonical execution interface with local and cloud adapters); QUAL-EV-0109 (the same fixture passes on local and cloud with equivalent effect and event semantics); docs/21 "Execution profiles".
- Qualification: one tool fixture run under `local_trusted` on a local Core (the host's workspace and terminal broker) and under `cloud_isolated` on a cloud worker (the sandbox through the gateway), compared call by call.
- Evidence tier: real-system.

## Goal

The tools are one interface: `shell.exec`, `fs.read`, `fs.list`, `fs.stat` (and the rest) mean the same thing whether the adapter is the host's workspace and `modbit-execd` or the task's sandbox — the same effect in the same fields, the same events per call, the same refusals.

## Existing-code audit

- classification: PARTIAL before this task: the adapters existed (M8.5 routed the sandboxed tools through `SandboxPort`) but their outputs diverged in shape (`name` vs `path` in listings, no `encoding` on a sandboxed read, no `path` on a sandboxed stat) and nothing compared them.
- production entry points: `crates/tools/src/direct.rs` (the sandbox adapter's `fs.list` entries now carry `path` as the host's do, `fs.read` carries `encoding`, `fs.stat` carries `path`; the adapter adds only its own identity, `sandbox`), `services/modbit-guest/src/serve.rs` (`fs.list` at the workspace root omits the block device's own `lost+found` on the MicroVM — a listing is workspace content on every backend; conformance step `fs_root_lists_workspace_content`), `crates/sandbox/src/port.rs` (the port the cloud adapter uses), `crates/tools/src/pipeline.rs` (the one pipeline both go through: proposal, validation, policy, dispatch, outcome — the same events).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_ev_0109_the_same_tool_fixture_runs_locally_and_in_the_cloud_with_equivalent_effect_and_event_semantics` — the fixture (a plan, a process writing and printing a file, a read, a listing, a stat, a read outside the root) on a real local Core and on a real cloud worker with a sandbox; per call the same status line, the same effect fields (`exit_code`, `stdout_preview`, `cancelled`, `timed_out`, `signal`; `path`, `content`, `byte_length`, `content_hash`, `truncated`, `encoding`; the listing's `path`/`kind`/`size` entries; the stat's `path`/`kind`/`size`), the same refusal (`PATH_OUTSIDE_ROOT`) for the read outside the root, and the same sequence of tool-call event types per call on both logs (`ToolCallProposed`, `ToolCallValidated`, `ToolCallPolicyDecision`, `ToolCallDispatched`, `ToolCallSucceeded` / `ToolCallFailed`).

## Limitations

- Adapter-specific fields differ and are documented as such: the host's `workspace_revision` and per-entry `content_hash`, the sandbox's `sandbox` id and `modified_ms`/`mode`.
- The sandbox enforces protected paths at the mount level for processes; the local trusted profile enforces them on the change path only — a process writing `.git/hooks` is refused in the sandbox and not on the host (docs/21, by design of the profiles).
- The fixture covers the file and process tools; browser tools are compared by their own qualifications (M7 locally, M8.8 in the cloud).

## Verification

- `qual_ev_0109_the_same_tool_fixture_runs_locally_and_in_the_cloud_with_equivalent_effect_and_event_semantics`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
