# Task Card — M5.2 Embedded QuickJS isolate with no ambient authority

## Identity

- Task ID: M5.2
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/16 "Procedural Tool Runtime" (`exec` runs JavaScript in an embedded QuickJS isolate with no network, filesystem, process or dynamic-module access; the only host bindings are capability-filtered `tools.*` async functions; CPU instruction/time, memory, call count and output budgets are enforced by the host; the isolate cannot bypass the Capability Kernel), REQ-EV-0231 (isolated code composition over governed `tools.*` with execution/time/output limits), docs/36 "Procedural isolate: DEPEND QuickJS via maintained Rust binding", docs/35 binding rule (a dependency never owns Modbit state, policy or semantics; replaceability behind a seam).
- Qualification: the isolate half of docs/51 `E2E-012` ("QuickJS has no direct fs/network/process access; CPU/memory/tool budget enforcement works"); the `tools.*`-through-the-registry half and the coding task belong to M5.3/M5.4 and QUAL-EV-0097/0231.
- Evidence tier: real-system (the real engine, real budgets tripped by real programs, a host that serves real async futures)

## Goal

Give the product an embedded JavaScript isolate that can run model-written programs with no ambient authority at all — the only way out is `tools.*`, every call of which the host serves and can refuse — under budgets the host enforces, so that M5.3 can route those calls through the Tool Registry and the Capability Kernel unchanged.

## Existing-code audit

- classification: NOT-FOUND before this task. `crates/procedural-runtime` was the M0.1 stub (no behavior); no JavaScript engine was in the workspace; docs/35 carried the binding as PROVISIONAL.
- production entry points:
  - `crates/procedural-runtime/src/lib.rs` — `Host` (bindings + `invoke(name, arguments_json) -> future<Result<json, HostError>>`), `Budget` (CPU deadline, interrupt-poll ceiling, heap, stack, tool calls, output bytes, log bytes; defaults 5 s / unbounded polls / 64 MiB / 1 MiB / 64 calls / 256 KiB / 64 KiB), `run(source, budget, host)` (dedicated thread, own single-threaded executor; the program is the body of an async function so `await`/`return` work at the top level), `run_async`, `Outcome { status: COMPLETED | FAILED | BUDGET_EXHAUSTED{budget}, value_json, error (constructor name + message + stack), log, log_truncated, calls, interrupt_polls, elapsed_ms, memory_peak_bytes }`, the prelude that builds a frozen `tools` tree from the host's names and removes the raw bindings, `DENIED_GLOBALS`, `ENGINE` ("quickjs-ng 0.16.2 via rquickjs 0.13.0").
  - `Cargo.toml` — `rquickjs = { version = "0.13.0", default-features = false, features = ["futures"] }` (MIT; no loader, no `std`/`os`, no dynamic loading); docs/35 records owner, license, source, confinement and exit plan.
- proof (`crates/procedural-runtime/tests/isolate.rs`, the real engine):
  - `the_isolate_has_no_ambient_authority` — every `DENIED_GLOBALS` name is `undefined`; the raw host bindings are gone after the prelude; `import('fs')` rejects; `new Function('return require')` cannot reach anything; `tools` can be neither replaced nor extended.
  - `tools_bindings_are_the_only_way_out_and_every_call_reaches_the_host` — a program composes `search.exact` → `fs.read` → `Promise.all` of reads → `test.run`; every call reaches the host in order with its JSON arguments; a tool the host did not offer is not a function; the outcome records the five calls and the log line.
  - `a_host_refusal_is_a_typed_error_the_program_may_catch_and_never_an_effect` — a host `POLICY_DENIED` is thrown as an `Error` with `code`/`tool`/`message` the program may catch and continue from; uncaught, it fails the program with the reason.
  - `the_cpu_budget_ends_a_spinning_program` — `while (true)` under a 300 ms budget ends as `BUDGET_EXHAUSTED CPU_TIME` well inside 5 s; a ceiling of 3 interrupt polls ends it after at most 4.
  - `the_memory_and_stack_budgets_end_a_program_that_grows_without_bound` — unbounded allocation under 8 MiB ends as `MEMORY`; unbounded recursion as `STACK`.
  - `the_tool_call_output_and_log_budgets_are_enforced` — the fourth call under a budget of three never reaches the host and ends the program as `TOOL_CALLS` (or is caught as `TOOL_CALL_BUDGET`); a 5000-byte return under a 1000-byte ceiling is `OUTPUT` with no value; log lines past the ceiling are dropped and the log says so.
  - `a_thrown_error_fails_the_program_with_its_message_and_stack`, `the_async_form_runs_on_its_own_thread_and_awaits_real_host_futures` (three concurrent 50 ms host futures take about 50 ms).

## Limitations

- The CPU budget is a wall-clock deadline checked at the engine's interrupt polls plus an optional poll ceiling; quickjs-ng exposes no exact instruction counter, so "instruction" budgets are poll-proportional, not exact.
- Nothing routes through the Tool Registry yet (M5.3); the `Host` in these tests is a table. `exec/wait/request_user_input` as model-visible tools are M5.4.
- `Date.now()` and `Math.random()` remain available (engine intrinsics); determinism of programs is not a claim of this task.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `crates/procedural-runtime/tests/isolate.rs` — the eight tests above

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
