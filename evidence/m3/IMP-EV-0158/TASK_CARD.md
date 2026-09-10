# Task Card — IMP-EV-0158 Commit history

## Identity

- Task ID: IMP-EV-0158
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0158; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0158` — Old commit cannot override current code truth.
- Evidence tier: real-system or production-equivalent

## Goal

Use Git history/blame as subordinate context with current-source priority.

## Existing-code audit

- classification: IMPLEMENTED by M3.6/M3.8 (was NOT-FOUND)
- production entry point: crates/git/src/lib.rs log_recent; graph cochange/owners/commits; pack excerpts from the active revision
- proof: history is a subordinate signal (co-change and ownership neighbours rank below the query's own hits) and pack bytes always come from the current worktree, never from a commit — an old commit cannot override current code truth

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `log_recent_lists_commits_with_their_paths_newest_first`
- `graph_answers_imports_importers_history_tests_evidence_and_refreshes_per_path`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
