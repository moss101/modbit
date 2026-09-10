# Task Card — IMP-EV-0261 Non-disruptive side question

## Identity

- Task ID: IMP-EV-0261
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-EV-0261 (ADAPT); owner label: Input Queue; subsystem: core-runtime
- Qualification: QUAL-EV-0261 — Ask side question mid-run; main state/event cursor remains unchanged.
- Evidence tier: real-system or production-equivalent

## Goal

Ask side question mid-run; main state/event cursor remains unchanged.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: services/modbit-core/src/side.rs AskSideQuestion: one model call over goal + plan + the last 12 transcript messages, no tools, nothing appended
- proof: the answer comes back with the route record; task state, loop and log offset are identical before and after; an empty question is refused before any model call

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0261_side_question_answers_from_a_snapshot_without_touching_task_state_or_cursor`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34437627213.json`, `ci-run-34437627213-tests.log`: hosted CI run 34437627213 on a1f7615, green on macOS, Linux and Windows
