# Task Card — IMP-EV-0231 Tool RPC composition / execute code

## Identity

- Task ID: IMP-EV-0231
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0231; owner label: Procedural Tool Runtime; subsystem: procedural-runtime
- Qualification: `QUAL-EV-0231` — Script attempts unauthorized tool and is denied by same Kernel.
- Evidence tier: real-system (E2E-012 on the real Core; the isolate tests on the real engine)

## Goal

Isolated code composition over governed `tools.*` with execution, time and output limits; the kernel, not the isolate, decides.

## Existing-code audit

- classification: NOT-FOUND before M5.2; this task is the qualification of the boundary.
- production entry points: `crates/procedural-runtime` (no ambient authority; budgets), `services/modbit-core/src/procedural.rs` (bindings = projection; the pipeline's policy stage — the same `KernelPort` — decides every call; the approval flow inside a program).
- proof: E2E-012: the program's `git.worktree.close` reaches the same kernel port and approval flow as a direct call and ends `APPROVAL_DENIED` with no effect; an out-of-plan write is refused by the harness gate inside the program; a tool the projection withholds is not even a binding (`the_isolate_has_no_ambient_authority`, `tools_bindings_are_the_only_way_out_…`); budgets: `the_cpu_budget_ends_a_spinning_program`, `the_memory_and_stack_budgets_…`, `the_tool_call_output_and_log_budgets_…`.

## Limitations

- A kernel denial of a projected tool for want of a lease capability is exercised through the approval path (denied) rather than a lease gap, since the projection already excludes what the lease lacks.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate`
- `the_isolate_has_no_ambient_authority`
- `the_tool_call_output_and_log_budgets_are_enforced`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
