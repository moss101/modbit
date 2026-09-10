# Task Card — PX-026 Alpha language baseline for TypeScript/JavaScript, Python and Rust

## Identity

- Task ID: PX-026
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-PX-026 (related REQ-EV-0010); owner: verification; docs/76 language and platform matrix
- Qualification: QUAL-PX-026 — on the ts-webapp, python-service and rust-cli fixtures: exact and BM25 retrieval, revision-bound edits and real compiler and test-runner evidence attributed to tasks; clients label the languages at the Alpha baseline, not Tier A; any structural or language-service claim in Alpha for these languages fails; an edit that corrupts encoding or line endings fails
- Evidence tier: real-system or production-equivalent

## Goal

Prove the three Alpha candidates at Tier C plus real compile and test evidence on the fixture stacks, with honest labels in every client.

## Existing-code audit

- classification: COMPLETED in this batch (was PARTIAL: retrieval, edits and cargo/vitest evidence existed; pytest evidence landed with PX-033; there were no labels and no line-ending guarantee)
- production entry point: `ListLanguages` → `LanguageList` (crates/protocol surface.proto; services/modbit-core/src/languages.rs catalog served by server.rs); CLI `language list`; desktop `languages:list` IPC and the header label (`language-labels`); crates/workspace `LineEnding` (edits and replacements keep a file's consistent CRLF/LF style; non-UTF-8 files refuse text edits)
- proof: the catalog labels typescript/javascript/python/rust `ALPHA_BASELINE` ("Tier A candidate at the Alpha baseline: Tier C plus compile and test evidence"), names the proven capabilities (exact/BM25 retrieval, revision-bound edits with preserved encoding and line endings, compile/test evidence attributed to tasks) with the real tests behind them, lists the M3.3–M3.7 context-engine capabilities as provisional and Tier A as explicitly not claimed; everything else is `UNSUPPORTED`; the Core test checks every cited evidence test exists in the repository; the CLI smoke prints the labels; the desktop E2E sees them; the workspace test proves an LF snippet spliced into a CRLF file stays CRLF, a whole-file LF replacement keeps CRLF (and LF stays LF), and a non-UTF-8 file refuses a text edit untouched

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_026_language_labels_are_honest_and_served_to_every_client`
- `cli_drives_a_real_core_end_to_end` (CLI `language list`)
- `edits_preserve_crlf_and_refuse_non_utf8`
- `vitest_fixture_parses_structured_reports_when_vitest_is_installed`, `pytest_fixture_produces_structured_reports_with_stable_failure_signatures_when_pytest_is_installed`, `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (compile/test evidence per stack)
- `m3_1_exact_regex_path_index_serves_bounded_revision_bound_hits_and_refreshes_on_writes`, `m3_2_bm25_lexical_index_ranks_files_and_refreshes_on_writes` (exact and BM25 retrieval)
- desktop `e2e/fleet.spec.ts` (the language label is visible)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
