# PX-178 task card — GUI task verifier and a failure judge that separates model, environment and policy outcomes

Decision Record: DR-PX-2026-10-26-018 (D1, D2). Evidence tier: release-critical (a new completion gate on effect-bearing GUI runs and new events on the task log).

## Audit (before)
Classification: DOCUMENTED-ONLY. The verification engine and the completion run judge code only: a GUI run with no file changes took the "nothing to verify" path and reached ReadyForReview whether or not the application showed the goal. `ComputerActionPerformed.verified` read back only a `set_value`. `FailureClass` had no model, environment or policy-stop outcome for a GUI run, and nothing on the log said why a GUI run failed. First missing link: no way to declare what a GUI task's done looks like, and nothing that read it.

## Change
- `crates/verification/src/gui.rs` (new, owner verification): `CompletionEvidence` (strict, `deny_unknown_fields`): `accessibility {app, role?, name, value}`, `file {path, contains}` (workspace-relative, no `..`), `recorded {event_type, field, equals}`; `eval_accessibility` reads only the declared application (a read of another one is MISSING, never a pass; an ambiguous or secure element is MISSING); `eval_file`, `eval_recorded`; `RunFacts` and `judge`: PASS → VERIFIED; otherwise environment facts → ENVIRONMENT_FAILURE (checked first), refusals → POLICY_STOP, else MODEL_FAILURE; every verdict carries its basis lines.
- `crates/computer/src/tree.rs`: `parse` reads back the lines `render` writes (handle, role, name, value or withheld), round-trip tested.
- `crates/domain`: `SubtaskSpec.completion_evidence` (the parent's declaration; the child never declares its own), `TaskEvent::GuiEvidenceEvaluated` and `TaskEvent::GuiOutcomeJudged` (no state change, JSON on the log, no proto or schema change).
- `services/modbit-core/src/gui_evidence.rs` (new): the Core reads the evidence itself — a `computer.state` observation of the task's open control session through the computer runtime under the task's mode and administrative policy (`ToolHost::verification_state_read`, never an input, never a new session), the workspace file through the path policy, or the task's own events; `run_facts` gathers latches, environment session closes, environment refusal codes of `computer.*` calls, denied or expired approvals, the person's stop.
- `runtime.rs`: `handle_complete` blocks completion with `COMPLETION_EVIDENCE_NOT_MET` (what the Core read and found; "your summary is not evidence") unless the evidence holds; at the run's end, before the control session closes, the evidence is read again and `GuiOutcomeJudged` is appended with the run's end.
- `spawn.rs`: a declaration that cannot be evaluated is refused at admission (`COMPLETION_EVIDENCE_INVALID`); `agent.spawn` documents and accepts `completion_evidence`.

## Proof
`services/modbit-core/tests/px_computer.rs` (real Core process, real fixture actuator process, real approvals by a person loop, a parent and a computer-use child on a scripted model):
1. `qual_px_178_a_completed_gui_task_passes_the_declared_evidence_and_is_judged_verified`: the order is submitted; the evaluation at completion is PASS and names the state read of the declared application; the verdict is VERIFIED, run end READY_FOR_REVIEW, envelope COMPLETED. A `file` predicate with `../` is refused at admission first.
2. `qual_px_178_an_unfinished_task_fails_its_evidence_whatever_the_model_claims`: Submit is never pressed and the summary claims "Status reads submitted"; completion is blocked with what the Core read (`reads "idle", not "submitted"`), no evaluation passes, the verdict is MODEL_FAILURE, the envelope is not COMPLETED.
3. `qual_px_178_a_run_that_clicks_the_wrong_element_is_a_model_failure`: the press on "Move the window" is delivered (SUCCEEDED) and the evidence still fails: MODEL_FAILURE with "no environment failure and no refusal" in its basis.
4. `qual_px_178_a_run_stopped_by_an_approval_denial_is_a_policy_stop`: the person denies the press; POLICY_STOP with the denied approval in its basis; the press never ran; every approval was resolved by the person.
5. `qual_px_178_a_run_whose_actuator_is_killed_is_an_environment_failure`: the actuator is SIGKILLed with an input in flight; ENVIRONMENT_FAILURE with the latch in its basis.
Unit: `modbit-verification` gui tests (strict parsing, workspace-relative paths, the declared application only, ambiguous and secure elements, the judge's order), `modbit-computer` `parse_reads_back_what_render_wrote`.
Mutation: `mutation-check.txt` — the judge ignoring environment facts fails test 5; a completion gate that never blocks fails test 2.

## Known limits
- Evidence is declared by a parent for a computer-use child (`agent.spawn`). A top-level GUI task has no declaration field: `CreateTask` would need a protocol change, not made here.
- An accessibility predicate needs the child's control session open on the application when it completes (it is told so when it is not); the Core never opens one itself.
- Facts are gathered over the task's whole log, not one run's slice: a child resumed after an environment failure that then fails on its own is still judged an environment failure.
- The browser stack is not covered: the predicate kinds read the computer runtime, the workspace and the log; a browser-run predicate is not built.
- PX-179 (the GUI regression suite) consumes this and is not built.
- Closed 2026-11-08: CI run 42421157777 (head 9d3e4e46) green on all 15 jobs, including the five qual_px_178 tests in px_computer on macOS, Linux and Windows (ci-run-42421157777.json). Landed on main as 1435ac81 (squash of moss101/modbit#174). Limits as above: parent-declared evidence only; no browser predicate; PX-179 not built.
