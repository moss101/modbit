# Task Card — IMP-EV-0238 Process-local background subagent durability limitation

## Identity

- Task ID: IMP-EV-0238
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0238; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0238 — Kill/restart test proves durability beyond process-local baseline.
- Evidence tier: real-system (a real Core process killed with SIGKILL mid-child and a second process on the same data directory, against a scripted OpenAI-compatible server)

## Goal

Modbit improves by persisting AgentNode/protocol state so background child survives Core lifecycle.

## Existing-code audit

- classification: MISSING before: a background child died with the Core process (the process-local baseline the source products share).
- production entry points: `runtime.rs::reconcile_after_restart` (protocol state and node suspension), `spawn::resume_suspended_children`.
- proof: a real Core process is killed with SIGKILL mid-child; a second process on the same data directory resumes the child on its own log with its identity, capsule and run, and the parent collects the result; nothing runs twice.

## Limitations

- See M6.7.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
