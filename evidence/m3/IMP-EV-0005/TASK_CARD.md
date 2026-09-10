# Task Card — IMP-EV-0005 Shared AST/symbol chunk representation

## Identity

- Task ID: IMP-EV-0005
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0005; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0005` — Cross-language fixture proves symbol identity is consistent across index/query/impact paths.
- Evidence tier: real-system or production-equivalent

## Goal

One structural representation feeds retrieval, prompt packing and impact analysis.

## Existing-code audit

- classification: IMPLEMENTED by M3.3/M3.5/M3.6/M3.8 (was NOT-FOUND)
- production entry point: crates/retrieval/src/symbols.rs (Symbol with path, kind, span, lines, content hash); chunk boundaries, pack signatures and evidence attribution all read it
- proof: one Symbol representation from the tree-sitter parse feeds search.symbols, the semantic chunk boundaries, the pack's stub signatures and the attribution of verification checks to test symbols (Rust, TypeScript, Python fixtures)

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m3_3_tree_sitter_symbol_index_serves_definitions_and_refreshes_on_writes`
- `symbol_index_extracts_alpha_language_definitions_and_refreshes`
- `semantic_index_finds_nearest_chunks_and_reembeds_only_changed_files`
- `m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence`
- `m2_8_verification_engine_gates_completion_on_real_cargo_fixture`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
