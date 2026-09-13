# Task Card — IMP-EV-0049 Agent parking

## Identity

- Task ID: IMP-EV-0049
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0049; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0049 — Park child during parent intervention, restart Core, resume from same state.
- Evidence tier: real-system (a real Core killed and restarted around a parked child, against a scripted OpenAI-compatible server)

## Goal

Park is durable state distinct from cancel/complete and preserves resumable context.

## Existing-code audit

- classification: MISSING before: a child could be cancelled (terminal) or left to a restart; nothing parked it by choice.
- production entry points: `services/modbit-core/src/runtime.rs` (`Running.park`, `Runtime::park`, `LoopEnd::Parked` at the turn boundary → `RunSuspended` + `TaskWaiting(External)`, agent end `PARKED`), `spawn::record_result` (`PARKED` envelope and node transition), `agent_tools.rs` (`agent.park`, `agent.resume` over `spawn::resume_child`; `result_of` ignores an interim envelope once the child is back; `wait_for` reports `PARKED` / `WAITING` instead of a result), `crates/tools/tool-matrix.json`, docs/17.
- proof: the parent parks a slow child during its intervention: the child stops at its next turn boundary before its write, its task reads Waiting/External/Suspended with no live loop, the node PARKED, a `PARKED` envelope on the parent; the Core is killed and restarted: the node is still PARKED with the same agent id and capsule; the parent resumes and the child stays parked until the parent's `agent.resume`; then it continues on its own run (`RunResumed`, no `RunCancelled`), writes once, and the parent collects a COMPLETED envelope; the node's history is ADMITTED→BACKGROUND→PARKED→BACKGROUND→COMPLETED.

## Limitations

- A park lands at the next turn boundary (a model call in flight finishes first); the parent parks and resumes — a user does it through the parent task.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0049_a_parked_child_survives_a_restart_and_resumes_from_the_same_state`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
