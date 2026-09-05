# Terminal, Execution Router, and Sandbox Architecture

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Execution profiles

- `local_trusted` — runs against user-approved local workspace under host policy.
- `cloud_isolated` — runs inside tenant-bound isolated MicroVM.
- Future profiles may be added through the same Execution Router; no tool changes required.

## Structured command contract

```text
ExecRequest {
  argv[]
  cwd
  env_handles[]
  timeout_ms
  pty: bool
  stdin_mode
  output_budget_bytes
  execution_profile
  capability_lease_id
  terminal_session_id?
}

ExecResult {
  process_id
  exit_code?
  signal?
  stdout_ref
  stderr_ref
  duration_ms
  terminal_cursor
  effect_receipt_id?
  checkpoint_before?
  checkpoint_after?
}
```

Shell-string convenience is parsed into an argv-aware request and clearly marked; internal deterministic tools prefer argv.

## Durable `modbit-execd`

A small broker owns local PTYs/processes and a bounded replay log. Core sends authenticated commands over local socket. UI can detach/reconnect without losing output. Core acknowledges output cursor; broker retains a sliding replay window and spills large output to OutputRef/object store. Broker is not authorized to create capabilities or decide policy.

## Command failure semantics

Non-zero exit is a valid tool result. It emits `CommandExited` with status and output; Agent Runtime may inspect, repair and retry. `ToolCallFailed` means execution infrastructure/schema/policy failure. `TurnFailed` occurs only when runtime can no longer make progress under policy/budget.

## Sandbox substrate boundary

Cloud isolated execution uses a Modbit-owned Sandbox Gateway in front of the MicroVM substrate:
- authenticated tenant-bound sandbox lease;
- deny-by-default sandbox-to-internal network;
- explicit egress domains/ports by capability;
- dynamic credential handles via broker injection;
- protected filesystem paths;
- typed guest RPC with task/turn/call/effect IDs;
- resource quotas and emergency stop;
- guest image is immutable/versioned and contains no tenant secrets.

## `modbit-guest`

Guest RPC methods are narrow: process start/wait/cancel, PTY attach/replay, file operations within mounted workspace, Git helpers, artifact upload/download, browser endpoint control and health. Gateway supplies a capability token scoped to sandbox lease and call.

## Handoff local → cloud

Handoff bundle contains immutable workspace checkpoint, Git metadata, context/index generation references, task/protocol state, tool capability requirements and encrypted/opaque secret handle references. It never contains raw secret values. Cloud admission verifies capability parity before switching execution owner.

## Sandbox recovery

If a sandbox dies, Core marks in-flight tool calls unknown, reconciles protected effects, provisions a fresh sandbox, restores latest valid checkpoint and resumes. External side effects are never replayed automatically.
