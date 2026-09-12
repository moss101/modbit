# Task Card — IMP-EV-0099 Command failure ≠ turn failure

## Identity

- Task ID: IMP-EV-0099
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0099; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0099 — Compile failure followed by fix/test succeeds in same run without task crash.
- Evidence tier: real-system (the real Core against a scripted OpenAI-compatible server; a real shell check and the real rust-cli cargo fixture)

## Goal

Nonzero exit is typed result and may feed repair loop.

## Existing-code audit

- classification: PRESENT since M2.7/M2.8 (docs/14 "Turn shape": a turn never ends because a tool or a test failed; command failure is evidence, not turn failure). No code change in this task: the audit traces the production path and names the qualification that already runs on every CI push.
- production entry points: `services/modbit-core/src/runtime.rs` (a `test.run` / `proc.exec` nonzero exit is a `ToolResult` with a failure signature in `open_failures`, never a `TaskFailed`; the repair loop, PX-018, consumes it), `crates/verification` (normalized failing checks, `Stage::Targeted` / `Completion`).
- proof: `m2_7…`: the check fails after the first edit (`sh check.sh` nonzero) and the same run edits again, the check passes and the task reaches ReadyForReview with no `TaskFailed` (asserted by name for REQ-EV-0099); `m2_8…`: on the real cargo fixture a failing TARGETED run reports normalized checks and the COMPLETION run attributes a REGRESSION and refuses completion until it is fixed — the run continues in both.

## Limitations

- A compile failure is exercised through cargo on the rust-cli fixture (M2.8) and a shell check (M2.7); the repair loop's bounds are PX-018's.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review`
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
