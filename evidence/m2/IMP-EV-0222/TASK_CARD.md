# Task Card — IMP-EV-0222 Structured AskUserQuestion

## Identity

- Task ID: IMP-EV-0222
- Milestone: M2 (backlog batch 2c)
- Requirement: REQ-EV-0222 (ADAPT); owner label: Approval/Question Service; subsystem: tool-runtime
- Qualification: QUAL-EV-0222 — Headless run returns NEEDS_INPUT rather than hanging.
- Evidence tier: real-system or production-equivalent

## Goal

Headless run returns NEEDS_INPUT rather than hanging.

## Existing-code audit

- classification: IMPLEMENTED in this batch
- production entry point: user.ask harness tool (services/modbit-core/src/runtime.rs handle_ask), UserQuestionAsked/Answered events, ListQuestions/RespondToQuestion commands; CLI exit code 2
- proof: a typed question suspends the run with the loop idle; the CLI's task run --wait exits 2 with state=Waiting wait_reason=UserInput; the answer resumes the run and reaches the model as the tool result

## Verification

Named tests (crate/test-binary :: test name), run on macOS, Linux and Windows by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it`
- `modbit-cli/headless` :: `qual_px_000_headless_cli_task_lifecycle`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
