# Task Card — IMP-EV-0004 Incremental Merkle repository indexing

## Identity

- Task ID: IMP-EV-0004
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0004; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0004` — Large-repo incremental edit updates only affected index segments and stays revision-correct.
- Evidence tier: real-system or production-equivalent

## Goal

Use subtree/content identity to avoid full rebuilds and bind index state to repo revision.

## Existing-code audit

- classification: IMPLEMENTED by M3.1/M3.5/M3.9 with per-file content identity (subtree hashing is not needed at the measured scale: incremental refresh of one file is ~90 ms against a ~2 s cold build)
- production entry point: crates/retrieval/src/index.rs (content hashes, refresh), lexical/symbols/semantic refresh; bench incremental timing
- proof: an incremental edit re-enters only the changed paths in every index at the new revision; results carry index_revision and workspace_revision; the benchmark reports cold versus incremental times

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `exact_regex_and_path_hits_are_bounded_and_revision_bound`
- `refresh_reindexes_exactly_the_changed_paths_at_the_new_revision`
- `semantic_index_finds_nearest_chunks_and_reembeds_only_changed_files`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
