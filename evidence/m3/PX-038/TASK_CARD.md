# Task Card — PX-038 Scope policy with bounded expansion and mandatory questions

## Identity

- Task ID: PX-038
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-038; owner: core-runtime; docs/28 §3 "Scope policy"
- Qualification: QUAL-PX-038 — the first PlanRecorded freezes the original write set; each PlanRevised carries a scope delta; Core counts out-of-plan files and revisions against the ScopePolicy bounds; beyond a bound or on an always_ask_paths match the next out-of-scope write is refused until a typed question offering continue, split or stop is answered, and a ScopeExpansionRecorded event carries the counters; in headless mode the policy default fails closed to Needs Attention; the scope metric is computed against the original plan. Negative proof: silent scope widening is rejected; a question auto-answered by the agent changes nothing; measuring scope against the last plan revision fails
- Evidence tier: real-system or production-equivalent

## Goal

Scope is bounded, not merely transparent: expansion beyond the original write set needs a decision, and the decision is recorded with the counters it was measured against.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was PARTIAL: PX-016 refused a write the current plan does not declare and PlanRevised already carried its delta and reason, but nothing bounded how far a plan could be revised)
- production entry point: `crates/core-runtime/src/harness.rs` `ScopePolicy` (Alpha defaults 2 out-of-plan files, 2 revisions, an always-ask list of migrations, CI configuration, lockfiles, dependency manifests and security/policy files, `headless_resolution` FAIL_CLOSED), `HarnessState::check_write` scope gate, `scope_counters`, `scope_resolution`; `services/modbit-core/src/runtime.rs` records `ScopeExpansionRecorded`, refuses with `HARNESS_SCOPE_QUESTION_REQUIRED`, applies the user's answer on resume and ends a headless run in Needs Attention; domain `TaskEvent::ScopeExpansionRecorded`
- proof: the interactive fixture task edits a file inside the original write set, revises the plan to add a lockfile, and is refused twice for the same path (retrying without an answer changes nothing) with a `ScopeExpansionRecorded QUESTION_REQUIRED` each time and no write; the typed question offers continue, split and stop; after the user answers `continue`, `ScopeExpansionRecorded CONTINUE` carries the answer and the counters measured against the original plan (0 out-of-plan files, 1 revision), and the write lands exactly once. The headless fixture task writes two files outside the original plan (inside the bound), and the third is refused with `FAIL_CLOSED`, no write, and `TaskNeedsAttention`; `task.complete` never runs

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_038_scope_expansion_is_bounded_asks_a_typed_question_and_fails_closed_headless`
- `qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening` (silent widening stays refused; the original write set is frozen by the first plan)

## Note recorded with this task

A FLAG-class diff invariant whose path the current plan declares is recorded at every stage but blocks completion only until the plan states why (docs/64 §4): without that, a planned and justified lockfile edit could never complete because the completion-stage whole-diff evaluation re-raises it.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside

## Regression — any answer while a scope question waited decided the expansion (2026-09-25)

- Evidence tier: release-critical (permissions and policy: the scope decision gates a write; a security boundary: who may decide; recovery: the decision is rebuilt from the log after a restart).
- Found in review of the EPR-008 DI-9 unlock, which requires `Actor::User` in the same `UserQuestionAnswered` arm of `rebuild`. The PX-038 arm's own comment said "a scope question is answered by the user, never by the agent (docs/28 §3)", but the arm set `state.scope_answer` from any `UserQuestionAnswered` on the task while `scope_question_pending` was non-empty. It ignored the event's actor and which question the answer was for. Only `server.rs` `RespondToQuestion` appends that event, as `Actor::User`, and it refuses a second answer to a question, so this is defense in depth rather than a live hole. QUAL-PX-038's negative proof "a question auto-answered by the agent changes nothing" had no test: the qualification only retried the write without an answer.
- Audit: IMPLEMENTED-PARTIAL. The unlock is production-wired (the answer reaches `rebuild`, the next turn records `ScopeExpansionRecorded`, and only CONTINUE unlocks the paths), but its input is not bound to its author or its question. First missing link: the `UserQuestionAnswered` arm of `rebuild` in `services/modbit-core/src/runtime.rs`.
- Fix (`rebuild` only, no new event or field): a `UserQuestionAsked` recorded while `scope_question_pending` is non-empty marks its `question_id` as a scope question. A `UserQuestionAnswered` sets `scope_answer` only when its actor is `Actor::User(_)` and it answers one of those questions (each is consumed by its first user answer), and a scope question is still pending. The answer still becomes the result of the model's `user.ask` call as before. The legitimate path is unchanged, because the loop records `ScopeExpansionRecorded QUESTION_REQUIRED` before the model can ask, and the run suspends on the question until the user answers it.
- Proof (real Core): `px_038_only_the_users_answer_to_the_scope_question_decides_the_expansion`, a desktop task on a real repository with a scripted model.
  - Phase 0: the task asks an unrelated question, which the user answers `continue` through `RespondToQuestion` before anything waits. Its write to the always-ask `pnpm-lock.yaml` is then refused (`ScopeExpansionRecorded QUESTION_REQUIRED`) and it asks the scope question.
  - Phase A: the Core is killed, and a `UserQuestionAnswered { option_id: continue }` for the scope question is appended as `Actor::Agent("solver:<task>")` through `modbit_event_store::EventStore::append`: a well-formed event on the task's hash chain that closes the question for the loop. The restarted Core resumes under a new lease and the model reads the `continue` answer. Its retried write is refused again with `HARNESS_SCOPE_QUESTION_REQUIRED`. The log holds two `QUESTION_REQUIRED` and no CONTINUE, and no `FileChanged` for the lockfile.
  - Phase B: the user answers the next scope question `stop` through `RespondToQuestion`. The Core is killed, and a `continue` to the earlier, unrelated question is appended with the user's own actor, copied from their real answer. The restarted Core records the scope question's answer: the resolutions are `QUESTION_REQUIRED, QUESTION_REQUIRED, STOP, QUESTION_REQUIRED`, the STOP carries `paths: [pnpm-lock.yaml]` and the answer `stop`, and the next write waits again. The lockfile is never written: no `FileChanged`, and its bytes are unchanged on disk.
- Regression: `qual_px_038_scope_expansion_is_bounded_asks_a_typed_question_and_fails_closed_headless` (the user's `continue` still records CONTINUE, and the write lands once), `qual_px_016_…`, and the other surface tests that answer typed questions (`qual_ev_0222_px_014_…`, `qual_ev_0077_0122_…`, `qual_ev_0118_…`, `qual_ev_0151_0275_…`, `qual_ev_0096_…`, `qual_ev_0174_…`, `qual_px_040_…`) pass. See `local-green-targeted.log`.
- Fault injection: the test was written and run before the fix, in two stages.
  - On the unfixed Core, phase A fails: after the agent's `continue`, the retried `change.apply` returned `status: SUCCESS` and wrote `pnpm-lock.yaml` (`local-red-unfixed.log`).
  - With the actor check alone, phase A passes and phase B fails: the resolutions are `QUESTION_REQUIRED, QUESTION_REQUIRED, CONTINUE`. The user's later `continue` to the earlier question overrode their `stop` to the scope question, and the lockfile was written (`local-red-actor-check-only.log`).
  - Only the full fix passes.
- Limitation, a follow-up and not part of this regression: `ScopeExpansionRecorded` is recorded under the run's agent actor (`solver:<task>`, `run_loop`), and `rebuild` applies a CONTINUE whatever its actor. So an event forged directly as `ScopeExpansionRecorded CONTINUE` would still unlock the path. Closing that means recording the decision as the Core, as DI-9's `ProtectedPathsUnlocked` is (`Actor::Core`), and honouring only Core-recorded decisions. That changes the event's recorded actor.
