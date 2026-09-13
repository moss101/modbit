# Task Card — IMP-EV-0151 Needs Attention UX

## Identity

- Task ID: IMP-EV-0151
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0151; owner label: Agent Runtime / desktop; subsystem: core (`attention.rs`), protocol, CLI, desktop renderer
- Qualification: QUAL-EV-0151 — Each attention reason is actionable and clears from canonical event.
- Evidence tier: real-system (the real Core with real approvals, questions, stalls and a one-slot capacity pool; the packaged Electron app against the real Core)

## Goal

Aggregate approvals, conflicts, failures, stalls and questions.

## Existing-code audit

- classification: PARTIAL before: the Fleet derived a Needs Attention column from `TaskNeedsAttention` and wait reasons in the renderer (M6.6); approvals and questions lived in their own lists and nothing aggregated them with an action.
- production entry points: `services/modbit-core/src/attention.rs` (`attention(store, session)` over `open_tasks`, approvals, questions, attention lines, admission refusals and protected effects), `server.rs` `GetAttention`, `crates/protocol` `AttentionView`, `apps/cli` `attention list`, desktop `attention:list` → `window.modbit.attention` → the attention strip in `index.tsx`.
- proof: in one session a question, an approval and a stall stand as three items ordered by the event that raised them, each naming its task, reference and the clearing command; approving clears the approval (and the effect runs), answering clears the question, cancelling clears the stall — nothing else does; on a one-slot host a refused start is a CAPACITY item until the task starts; the packaged app renders the CAPACITY item from the Core and shows it gone when the second task starts.

## Limitations

- CONFLICT and PROTECTED_EFFECT items are covered by their own tests' events (M6.3 / REQ-EV-0046) and the projection rules, not by a timing-dependent E2E; the strip is a list with reasons and actions in words — the acting controls stay where they are (review, approvals, start).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0151_0275_attention_items_are_derived_from_canonical_state_and_clear_with_it`
- `attention: a start refused for capacity is an item from the Core and clears when the task starts (apps/desktop/e2e/attention.spec.ts)`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
