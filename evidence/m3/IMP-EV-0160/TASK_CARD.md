# Task Card — IMP-EV-0160 Editor/active state context

## Identity

- Task ID: IMP-EV-0160
- Milestone: M3
- Requirement: REQ-EV-0160; owner label: Workspace Context Bridge; subsystem: context-engine
- Qualification: `QUAL-EV-0160` — Selection influences retrieval and is visible in inspector.
- Evidence tier: real-system or production-equivalent

## Goal

Use active review/file/symbol/task selection without needing an IDE.

## Existing-code audit

- classification: NOT-FOUND before this task. The review surface knew which hunks a reviewer rejected and nothing else; no client could tell the task what the user was looking at.
- production entry point: `SetTaskSelection` → `TaskEvent::SelectionRecorded`; `services/modbit-core/src/tools.rs` (`Selection`, `selection_of`, the pack's task constraints); `services/modbit-core/src/inspector.rs`; `services/modbit-core/src/runtime.rs` (the selection in the harness state the model sees); desktop review screen (`hunk-context`, `review-selection`); CLI `task select`.
- proof: a selection is set through the protocol from any client — the CLI needs no IDE — and it changes what retrieval returns: the selected paths become critical entries in the Context Pack with the source `selection` and a reason naming the surface that chose them, and a selected line range is the excerpt that enters. The Context Inspector reports the selection itself (paths, symbol, source, review hunks) next to the entries, so a reader can see why a fragment is in the pack. When a wider retrieval hit covers a selected range the pack keeps the wider text and the entry still declares both sources, so absorption never loses the provenance.

## Limitations

The selection is one per task: setting a new one replaces the old, and there is no history of what was selected when. A selected symbol is carried and shown but does not yet narrow the excerpt to that symbol's span; a selected line range does. No editor plugin reports an active file yet, so `editor` is a source label the protocol accepts rather than an integration that exists.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0141_0160_a_selection_steers_retrieval_is_visible_and_grants_no_write`
- `duplicates_collapse_on_span_identity_or_containment`
- `qual_ev_0035_0131_0175_context_inspector_matches_the_prompt_envelope`

The desktop review surface is exercised by `review.spec.ts` in the same CI run.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
