# Task Card — IMP-EV-0179 Background subagents with follow-up continuation

## Identity

- Task ID: IMP-EV-0179
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0179; owner label: Agent Runtime; subsystem: core-runtime
- Qualification: QUAL-EV-0179 — Complete child, restart parent, send follow-up and verify lineage/state.
- Evidence tier: real-system (a real Core killed and restarted between a child's completion and the parent's follow-up, against a scripted OpenAI-compatible server)

## Goal

Background child remains addressable/resumable; follow-up is typed and prior output treated as evidence.

## Existing-code audit

- classification: PARTIAL before: a background child was resumable after a restart (M6.7) but not addressable once it had ended.
- production entry points: `spawn::follow_up_child` (live: `TaskInputQueued` STEER | FOLLOW_UP; parked / suspended: input + `resume_child`; ended: new attempt), `agent_tools::handle_steer`.
- proof: the child completes, the Core is killed and restarted, the parent resumes (the finished child untouched by the restart: still COMPLETED, same agent id) and its re-asked turn sends the typed follow-up; the child continues on its own log with its prior output as context and evidence, ends a second time, and the parent collects the linked envelope.

## Limitations

- A follow-up to a live child is delivered as a typed input; the parent reads the effect from the child's next envelope.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0050_0179_a_follow_up_continues_a_finished_child_as_a_new_attempt_on_its_lineage`
- `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
