# Task Card — IMP-EV-0275 Reminder/decision engine

## Identity

- Task ID: IMP-EV-0275
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0275; owner label: Agent Runtime / desktop; subsystem: core (`attention.rs`), protocol, CLI, desktop renderer
- Qualification: QUAL-EV-0275 — Attention item is created/cleared solely from canonical unresolved state.
- Evidence tier: real-system (the real Core; the packaged Electron app)

## Goal

Host derives actionable reminders from unresolved approvals/questions/blockers/deadlines; no second scheduler.

## Existing-code audit

- classification: MISSING before: no reminder derivation existed; the renderer flagged tasks from events it happened to see.
- production entry points: `services/modbit-core/src/attention.rs` (a pure derivation over the log and projections: no stored items, no timers, no scheduler), `GetAttention`, the desktop strip re-read after every task event.
- proof: every item exists exactly while the canonical fact it names is unresolved: an approval item is gone with `ApprovalResolved`, a question item with `UserQuestionAnswered` (even though the attention line that announced it still stands on the log), a stall item with `TaskCancelled`, a capacity item with `TaskStarted`; a fresh session has none; nothing is created by the client.

## Limitations

- Deadlines are not a canonical fact on the log yet (no due-by on tasks); an item for one arrives when the Core records them.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0151_0275_attention_items_are_derived_from_canonical_state_and_clear_with_it`
- `attention: a start refused for capacity is an item from the Core and clears when the task starts (apps/desktop/e2e/attention.spec.ts)`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
