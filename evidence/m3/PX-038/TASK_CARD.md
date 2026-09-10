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
