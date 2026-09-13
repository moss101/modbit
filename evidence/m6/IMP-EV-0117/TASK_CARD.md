# Task Card — IMP-EV-0117 Plan mode

## Identity

- Task ID: IMP-EV-0117
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0117; owner label: Agent Runtime / Capability Kernel; subsystem: policy, tools, core (`plans.rs`), protocol, CLI
- Qualification: QUAL-EV-0117 — Attempt write in Plan mode is absent/denied before execution.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; real leases and plan objects)

## Goal

Plan profile is mutation-disabled and may require review by risk/policy.

## Existing-code audit

- classification: MISSING before: no read-only execution profile existed; a plan-only task had to be a normal task that happened not to write.
- production entry points: `crates/policy/src/kernel.rs` (`PROFILE_PLAN`, ceiling `READ_ONLY`, lease `fs.read` / `git.read`), `crates/tools/src/direct.rs` (`plan` among the served profiles), `crates/tools/src/policy.rs` (`PLAN_MODE` deny for any non-read effect under `plan`), the compiled surface (`visible_specs`) which omits what the lease ceiling denies.
- proof: a task created under `plan` is offered reads and the harness and no `change.apply` / `change.batch` / `fs.write` / shell / worktree tool; the write the script asks for anyway is refused before the kernel (no `ToolCallProposed`), the file never appears, the plan is `PlanRecorded` v1, the lease says `READ_ONLY`, and the task reaches Ready for Review with an empty review bundle — the plan is the product.

## Limitations

- Whether a plan-mode result needs a human review before execution is the review surface's existing decision (every candidate is reviewed); no separate "execute this plan" command exists — a person starts a normal task with the reviewed plan (REQ-EV-0118).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0117_plan_mode_has_no_write_and_its_plan_goes_to_review`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
