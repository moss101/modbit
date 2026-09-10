# Task Card — IMP-EV-0060 Repository Knowledge Artifact / Wiki

## Identity

- Task ID: IMP-EV-0060
- Milestone: M3
- Requirement: REQ-EV-0060; owner label: Repository Knowledge; subsystem: context-engine
- Qualification: `QUAL-EV-0060` — Edit source after wiki generation; stale claim flagged and never treated as authority.
- Evidence tier: real-system or production-equivalent

## Goal

Generated architecture summaries are cache/discovery aids and always source-checked.

## Existing-code audit

- classification: NOT-FOUND before this task. The indexes knew the symbols, the imports and the tests of every file; nothing turned that into a map a reader could ask for, and nothing would have aged it.
- production entry point: `crates/retrieval/src/knowledge.rs` (`build`, `check`, `AUTHORITY`); the `knowledge` search kind in `services/modbit-core/src/tools.rs` with the per-workspace cache; the `knowledge.map` tool in `crates/tools/src/direct.rs`.
- proof: the map is generated from the same indexes retrieval uses — what each module holds, exports, imports, is imported by and is tested by — and every claim carries the files it came from with the content hash each had when the claim was written. It is written once and kept as a cache; every read checks each claim against the files as they are now and returns it marked fresh, stale (with the hashes that moved) or missing. A real run proves the aging: the agent builds the map, edits a file it described, asks again, and the claims derived from that file come back stale naming `src/money.rs changed (…)` while claims about untouched modules stay fresh. The map carries its own limits in the payload: a sentence that says it is a discovery aid and a cache, never authority, that every claim names its files, and that a stale claim is not evidence.

## Limitations

Claims are structural, not semantic: the map says what a module holds, exports, imports and is tested by, not what it means or why. A module is a directory. Freshness is per source file, so a claim about a module goes stale when any file it names changes, even if the claim's own subject did not. The map is rebuilt only on request (`refresh`), never on a schedule.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0060_0203_the_repository_map_flags_stale_claims_and_never_enters_the_prompt_by_itself`
- `the_map_names_modules_exports_dependencies_and_its_own_sources`
- `a_claim_whose_source_moved_is_stale_and_one_whose_source_is_gone_is_missing`
- `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
