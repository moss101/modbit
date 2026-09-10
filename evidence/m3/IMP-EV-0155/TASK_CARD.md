# Task Card — IMP-EV-0155 Code structure mapping

## Identity

- Task ID: IMP-EV-0155
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0155; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0155` — Cross-file query resolves structural path correctly.
- Evidence tier: real-system or production-equivalent

## Goal

Represent files/modules/symbols/contracts/dependencies.

## Existing-code audit

- classification: IMPLEMENTED by M3.3/M3.6 (was NOT-FOUND)
- production entry point: crates/retrieval/src/graph.rs (imports/importers by language convention, sibling crates), symbols.rs
- proof: files, symbols and import dependencies are represented; a cross-file query (importers of a path, to depth 3) resolves the structural path in Rust, Python and TypeScript fixtures and in the real repository corpus

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence`
- `graph_answers_imports_importers_history_tests_evidence_and_refreshes_per_path`
- `specifiers_resolve_to_workspace_paths_per_language_convention`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
