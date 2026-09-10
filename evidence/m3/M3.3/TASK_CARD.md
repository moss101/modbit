# Task Card — M3.3 tree-sitter AST/symbol index

## Identity

- Task ID: M3.3
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `symbols` module; `search.symbols` tool in `crates/tools`; per-workspace symbol index and freshness in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Index inputs" (tree-sitter AST and symbol definitions), "Concrete local indexing stack" (Syntax: tree-sitter parsers for supported languages), L0/L2 retrieval; docs/76 Alpha languages (TypeScript/JavaScript, Python, Rust); docs/36 DEPEND: tree-sitter; docs/43 M3.3; REQ-EV-0005 / REQ-EV-0155 groundwork
- Risk class: medium (untrusted source parsed by C grammars in-process)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

Definitions of the Alpha languages extracted with the real tree-sitter grammars (rust 0.24, typescript/tsx 0.23, javascript 0.25, python 0.25) into typed symbols (kind, name, container, line range, byte span) bound to the file content hash and the index revision; unsupported languages contribute nothing and no error; refreshed per changed path on every write and rebuilt with the exact index when the workspace revision moved. Exposed as `search.symbols` (exact name or prefix, optional kind and path glob, bounded).

## Non-goals

References/usages and cross-file resolution (headless LSP, M3.4), the dependency/call graph (M3.6), chunking for embeddings (M3.5), languages beyond the Alpha baseline (docs/76 tiers are earned by evidence).

## Existing-code audit

- classification: NOT-FOUND (no parser or symbol index existed)
- production entry point: `modbit_retrieval::SymbolIndex` (build / refresh / query / symbols_in); `ToolHost::symbols`; `IndexPort` serves kind `symbols`
- real effector/storage boundary: real grammars parsing the real files of the exact index, in memory per Core process

## Invariants

- A symbol is claimed only where a grammar exists; other languages return no symbols.
- Every symbol carries the content hash of its file and the index revision; a rewritten file replaces its symbols at the new revision, a removed file leaves the index.
- Queries are bounded and path globs are validated (typed failure, never a panic).

## Verification

- `modbit-retrieval/index` :: `symbol_index_extracts_alpha_language_definitions_and_refreshes`
- `modbit-core/surface_protocol` :: `m3_3_tree_sitter_symbol_index_serves_definitions_and_refreshes_on_writes`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `ci-run-34444133255.json`, `ci-run-34444133255-tests.log`: hosted CI run 34444133255 on 06b5702, green on macOS, Linux and Windows
