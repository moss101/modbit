# Task Card — PX-014 Understanding and planning contract

## Identity

- Task ID: PX-014
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-PX-014 (ADOPT); owner label: core-runtime; subsystem: core-runtime
- Qualification: QUAL-PX-014 — Real fixture task: the agent records a plan through plan.update before the first write, asks exactly one typed question on an ambiguous fixture and none on an unambiguous one, and plan revisions appear as events in the timeline
- Evidence tier: real-system or production-equivalent

## Goal

Real fixture task: the agent records a plan through plan.update before the first write, asks exactly one typed question on an ambiguous fixture and none on an unambiguous one, and plan revisions appear as events in the timeline

## Existing-code audit

- classification: FOUND (plan gate and plan events since M2.7) + IMPLEMENTED (typed questions and the clarification flag)
- production entry point: plan.update before the first write (harness plan gate, M2.7), user.ask with the docs/28 clarification policy flag, PlanRecorded/PlanRevised events
- proof: ambiguous fixture: exactly one typed question, plan before the first write, a PlanRevised event after the answer; unambiguous fixture: no question; a yes/no question naming an existing file is flagged CONFIRMS_REPOSITORY_FACT; a write before any plan is refused (M2.7 test)

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it`
- `modbit-core/surface_protocol` :: `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
