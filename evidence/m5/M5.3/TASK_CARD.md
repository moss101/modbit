# Task Card — M5.3 Generated `tools.*` bindings routed through the normal Tool Registry

## Identity

- Task ID: M5.3
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/16 "Procedural Tool Runtime" (the only host bindings are capability-filtered `tools.*` async functions generated from ToolSpecs; the isolate cannot bypass the Capability Kernel: each binding is a normal tool call with ToolCallId, policy check and receipt), REQ-EV-0231 (QUAL-EV-0231: a script attempts an unauthorized tool and is denied by the same Kernel), REQ-EV-0097 (every nested effect policy/evidence-tracked), docs/14 harness contracts (plan, scope, retrieval, repair gates), docs/64 §1 (BASELINE before the first write).
- Qualification: docs/51 `E2E-012` — "`tools.*` calls produce normal ToolCall events/effects"; QUAL-EV-0231.
- Evidence tier: real-system (the real Core, the real pipeline and kernel, a real approval the user denies, the real workspace)

## Goal

Make a program's `tools.*` calls indistinguishable on the log and at the policy boundary from the model's direct calls: one tool-call aggregate per call under the exec call's id, the turn's projection fence, the Capability Kernel's decision, the approval flow with the task waiting on it, the receipt — and the harness write gates the model would have met.

## Existing-code audit

- classification: NOT-FOUND before this task. The isolate (M5.2) had a `Host` seam and a table host in tests; nothing routed a program's call through the Tool Registry.
- production entry points:
  - `services/modbit-core/src/procedural.rs` — `ProgramHost` (`Host` for the isolate): `bindings_for(projection, declared_effects)` (the turn's projection minus the harness's own tools, narrowed to the declared tool names/toolsets); `invoke` refuses a name outside the bindings (`TOOL_NOT_PROJECTED`), applies the doc 14 gates from the harness state the program started under (`HARNESS_PLAN_REQUIRED`, `HARNESS_PLAN_REVISION_REQUIRED`, `HARNESS_REPAIR_ATTEMPT_REQUIRED`, `HARNESS_SCOPE_QUESTION_REQUIRED` — a program cannot ask), then `serve`s the call on the Core's executor through `ToolHost::invoke` with a fresh `ToolCallId`, `call_id = <exec>#<n>`, the run/turn lineage, the lease generation and the turn's projection; an `ApprovalPending` result records `TaskWaiting(Approval)`, waits for the decision, records `TaskResumed` and re-enters with the same id — `APPROVAL_DENIED` to the program when the user says no; success returns the tool's structured output with `tool_call_id`, `stdout_ref` and `workspace_revision_after`; any other status is the tool's error code and message.
  - `crates/procedural-runtime` — `HostFuture` is `Send + 'static`, so the host bridges to the Core's multi-threaded executor from the isolate's own thread.
- proof: `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate` — the program's five calls (`search.exact`, `fs.read`, `change.apply`, `shell.exec`, `git.worktree.close`) are five `ToolCallProposed` under `call_1_0#1..5`; the out-of-plan `change.apply` is refused by the host gate with `HARNESS_PLAN_REVISION_REQUIRED` and never becomes a tool call (exactly one `change.apply` on the log; `c.txt` never exists); the destructive call opens a real approval (`git.worktree.close`), the task waits (`TaskWaiting` / `TaskResumed`), the user denies it, the program sees `APPROVAL_DENIED`, and the call's trail carries `ToolCallApprovalRequested`, never `ToolCallDispatched`, and ends `ToolCallFailed`; the program's write carries the workspace revision (2) and lands (`b.txt`); a program that may write ran the BASELINE first.

## Limitations

- A program cannot ask the user; a scope question inside a program refuses the write and the model asks between programs.
- The retrieval-before-edit rule is enforced through the pipeline's own ledger (a program's `fs.read` is a retrieval record like any other); the host does not pre-check it.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate` (services/modbit-core, real Core; E2E-012)
- `crates/procedural-runtime/tests/isolate.rs` (the seam: `tools_bindings_are_the_only_way_out_and_every_call_reaches_the_host`, `a_host_refusal_is_a_typed_error_the_program_may_catch_and_never_an_effect`)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
