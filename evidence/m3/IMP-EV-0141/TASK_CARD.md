# Task Card — IMP-EV-0141 Editor context bridge

## Identity

- Task ID: IMP-EV-0141
- Milestone: M3
- Requirement: REQ-EV-0141; owner label: Workspace Context Bridge; subsystem: context-engine
- Qualification: `QUAL-EV-0141` — Review selection affects context but cannot mutate canonical source.
- Evidence tier: real-system or production-equivalent

## Goal

In clean-slate Modbit this means selected/open review artifacts and active file/symbol context, not IDE ownership.

## Existing-code audit

- classification: NOT-FOUND before this task. The review surface knew which hunks a reviewer rejected and nothing else; no client could tell the task what the user was looking at.
- production entry point: `SetTaskSelection` → `TaskEvent::SelectionRecorded`; `services/modbit-core/src/tools.rs` (`Selection`, `selection_of`, the pack's task constraints); `services/modbit-core/src/inspector.rs`; `services/modbit-core/src/runtime.rs` (the selection in the harness state the model sees); desktop review screen (`hunk-context`, `review-selection`); CLI `task select`.
- proof: a reviewer marks hunks as context in the Review screen; the Core records a durable SelectionRecorded event under the session lease, and from then on every Context Pack for that task treats the selected paths as task constraints with their own source and reason. The selection is context and nothing else: the same run's write to the selected file is still refused because the plan does not declare it, and the file on disk is unchanged. The model is told what is selected, with the sentence that it grants no tool and no write.

## Limitations

The desktop bridge covers review hunks; an editor plugin that reports the active file and cursor is not built, so `editor` is a source label the protocol accepts rather than an integration that exists. Line ranges apply to the first selected path.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0141_0160_a_selection_steers_retrieval_is_visible_and_grants_no_write`
- `duplicates_collapse_on_span_identity_or_containment`
- `qual_ev_0035_0131_0175_context_inspector_matches_the_prompt_envelope`

The desktop review surface is exercised by `review.spec.ts` in the same CI run.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
