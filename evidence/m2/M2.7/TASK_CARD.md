# Task Card — M2.7 Basic Prompt Compiler and one-agent runtime

## Identity

- Task ID: M2.7
- Milestone: M2
- Canonical owner: core-runtime (`crates/core-runtime` harness contracts; the Core's `runtime` loop) and context-engine (`crates/prompt-compiler`)
- Requirement IDs: REQ-EV-0099 (command failure is evidence, not turn failure), REQ-EV-0107 (bounded failure evidence with declared truncation), REQ-EV-0044 (only projected tools reach the model); docs/14 "Main runtime loop" steps 5–9 and "Agent harness contracts (PX-040)" 1, 2, 4, 5, 9, 11; docs/28 PX-014 plan-before-write, PX-019 self-review, PX-039 no-progress detection; docs/15 "Prompt cache economics"; docs/43 M2.7.
- Risk class: high (drives real effects from model output)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

An event-driven one-agent loop: each turn is `ContextCompile → ModelInvoke → validated actions → tool execution → observation`, every step persisted as a RunStep before the next begins; the prompt compiler emits stable segments with hashes (system/policy, workspace rules, compaction epoch, task context pack carrying `harness_state`, recent events) and a tool projection hash; the model streams through the Provider Gateway; requested actions run through the tool host (kernel, approvals, receipts); observations are bounded with declared truncation; the plan gate, budgets, no-progress detection and the completion handshake are enforced by the Core; steering and cancellation apply at safe boundaries; a restarted Core suspends interrupted runs and resumes from the log without repeating a tool action.

## Non-goals

- Context Engine retrieval, Context Pack ledger and retrieval-before-edit enforcement (M3, PX-015).
- Verification engine stages, normalized CheckResults beyond the HEURISTIC configured command, regression attribution (M2.8).
- Repair attempt records, hypothesis equivalence, scope questions (PX-018/038 follow-ups), typed user questions, headless policy resolution.
- Routing, RequestProfile, ConditionalExecutionPlan (docs/27, EPR).
- A live model: the milestone proof "E2E-001/002/003 with live model" is tracked by DR-M2-001's live workflow; here the model is a scripted wire-faithful OpenAI endpoint.

## Required reading

`AGENTS.md`, `docs/13`, `docs/14`, `docs/15`, `docs/16`, `docs/28`, `docs/30`, `docs/decisions/DR-M2-001`.

## Existing-code audit

- classification: NOT-FOUND (`crates/core-runtime` and `crates/prompt-compiler` were M0.1 shells; no loop existed)
- production entry point: Core `StartTask` (Queued or Waiting task, lease-fenced) → `runtime::run_loop`; `CancelTask`; `GetTaskStatus`; CLI `task run [--wait]`, `task cancel`, `task status`
- real effector/storage boundary: Provider Gateway (real HTTP), tool host (real workspace/git/broker), event store (Run/Turn/RunStep/Task events; transcript entries and plans as content-addressed objects)

## Invariants

- No action before its assistant message is persisted; no next step before the current step's outcome is on the log.
- A write tool without a recorded plan is refused by the harness with evidence and never reaches the kernel.
- A failing command or test is an observation carrying a failure signature; the task never fails because of it; completion is refused while a signature is open or the self-review has unresolved findings.
- Budgets exhaust into `HarnessBudgetExhausted` + `Waiting(UserInput)` + `TaskNeedsAttention`; no silent truncation.
- Steering inputs become `TaskSteered` between steps and reach the model as tagged user messages; cancellation interrupts the in-flight stream and ends with `TurnInterrupted`/`RunCancelled`/`TaskCancelled`.
- Restart: `Running` tasks are suspended with attention at boot; `StartTask` on a `Waiting` task resumes the suspended run under a lease generation that never goes backwards, with the transcript rebuilt from step outputs.
- Broker idempotency keys include the tool call id: a new call never replays an older process.

## Verification

- Core (real socket, real repository, real broker, scripted model over real HTTP): `m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review` (read → plan → edit → failing check → fix → passing check → completion; 7 turns, 5 tool calls, 1 plan, 1 self-review, file changed, no TaskFailed), `m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart` (plan gate, max_turns exhaustion, SIGKILL mid-stream → suspended → resumed with 3 rebuilt tool results, one change.apply ever), `m2_7_steering_and_cancellation_apply_at_safe_boundaries`
- Harness unit tests (plan gate, scope freeze, completion handshake, budgets, observation truncation, failure signatures); prompt compiler unit test (stable segment hashes and projection hashes)
- Tools: identical `test.run` under a new call id runs again; the same id replays
- CLI smoke: `task status`
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
