# Task Card — M5.4 exec / wait / request_user_input surface

## Identity

- Task ID: M5.4
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/16 "Procedural Tool Runtime" (for eligible tasks the model-visible surface can be reduced to `exec(program, declared_effects, budget)`, `wait(handle, timeout)`, `request_user_input(question, schema)`), REQ-EV-0097 (minimal code-mode interface: exec / wait / request_user_input; QUAL-EV-0097: a real coding task completes through procedural mode and every nested effect is policy/evidence-tracked), docs/14 (the loop's budgets, progress and completion handshake apply to programs).
- Qualification: docs/51 `E2E-012` in full — "QuickJS has no direct fs/network/process access; `tools.*` calls produce normal ToolCall events/effects; CPU/memory/tool budget enforcement works; final code/test result succeeds".
- Evidence tier: real-system (the real Core with the real isolate, real tools, a real approval, a real budget trip)

## Goal

Give the model the procedural surface as ordinary harness tools — `proc.exec`, `proc.wait`, and `user.ask` as `request_user_input` — so that one turn can compose many governed tool calls, with the program's lifecycle on the log and its effects folded into the task's own state.

## Existing-code audit

- classification: NOT-FOUND before this task. No model-visible way to run a program existed.
- production entry points:
  - `services/modbit-core/src/runtime.rs` — `proc.exec` and `proc.wait` in every projection (the exec description names the program's bindings for this turn); loop arms: `proc.exec` runs the BASELINE first when the bindings include a write tool and the plan exists (docs/64 §1), then `handle_exec`; `proc.wait` → `handle_wait`; `task.complete` refused while a program runs (`PROGRAM_RUNNING`); every program still running when the loop ends is cancelled; exec/wait are `ProcedureRun` steps.
  - `services/modbit-core/src/procedural.rs` — `ExecArgs { program, declared_effects, budget }`, `budget_for` (defaults capped by the ceilings CPU 120 s / heap 256 MiB / output 1 MiB and by the task's remaining tool budget), `Programs` (running by handle, finished outcomes), `handle_exec` (records `ProgramStarted` with the program object, bindings and budget; spawns `run_async`; returns the outcome within a 2 s grace or `RUNNING` with the handle), `handle_wait` (the outcome once ended — again for a finished handle — or `RUNNING` after the timeout, 60 s at most), `finalize` (the program's calls are the task's tool calls; its successful writes are noted for scope and set the candidate revision; its failed checks open failure signatures and its passing checks clear them; the outcome is a content-addressed object; `ProgramEnded` with status, exhausted budget, calls, elapsed time and interrupt polls), the bounded rendering the model sees.
  - `crates/domain/src/task.rs` — `TaskEvent::ProgramStarted` / `ProgramEnded` (record-only).
  - `crates/procedural-runtime` — `Cancel` (the host ends a program at its next interrupt poll or binding call), the CPU deadline extended by time spent awaiting the host (an approval is not CPU time).
- proof: `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate` — the model sees `proc.exec` with the bindings named; the first program (search → read → write → shell, an out-of-plan write refused, a destructive call awaiting approval) comes back `RUNNING` with its handle, `proc.wait` brings `COMPLETED` with the value (`probe` shows `require`/`process`/`fetch` undefined, one hit, the content, revision 2, exit 0, the two refusal codes, the `tool_call_id` on the write) and the log line; the second program spins past a 300 ms CPU budget and comes back `BUDGET_EXHAUSTED CPU_TIME`; the log carries two `ProgramStarted` / `ProgramEnded` (COMPLETED with 5 calls; BUDGET_EXHAUSTED CPU_TIME), three `ProcedureRun` steps, the bindings without `lsp.*` or `plan.update`; `b.txt` holds the program's write and the task reaches ReadyForReview.

## Limitations

- `request_user_input` is the existing `user.ask`, callable by the model between programs, not from inside one.
- A "procedural mode" that removes the direct tools from the projection for eligible tasks is not switched on: both surfaces are projected; the mode comparison is M5.6's benchmark.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate` (services/modbit-core, real Core; E2E-012)
- `time_awaiting_the_host_is_not_cpu_time_and_the_host_can_cancel` (crates/procedural-runtime)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
