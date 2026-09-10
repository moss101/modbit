# Task Card — M3.2 Tantivy BM25

## Identity

- Task ID: M3.2
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `lexical` module; `search.lexical` tool in `crates/tools`; per-workspace BM25 index and freshness in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Index inputs" (lexical token index BM25), "Concrete local indexing stack" (Tantivy index per repository snapshot family), "Index freshness"; docs/36 DEPEND: Tantivy; docs/43 M3.2; REQ-EV-0154 groundwork (lexical side of fusion)
- Risk class: medium (model-supplied queries; untrusted content)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

A BM25 lexical index (Tantivy 0.25, in-memory per Core process) over the same indexed file set as the exact index: one document per searchable file, the default tokenizer so identifiers split into their words, terms AND-ed, lenient query parsing, ranked file hits with scores bound to the index revision; refreshed per changed path on every write and rebuilt with the exact index when the workspace revision moved without a recorded change set. Exposed as `search.lexical` (ReadOnly, `fs.read`).

## Non-goals

Snippets/positions from BM25 (exact search provides spans), on-disk index persistence across Core restarts (rebuilt at first use), semantic embeddings (M3.5), fusion (M3.7).

## Existing-code audit

- classification: NOT-FOUND (no lexical index; Tantivy was not a dependency)
- production entry point: `modbit_retrieval::LexicalIndex` (build / refresh / search); `ToolHost::lexical`; `IndexPort` serves kind `lexical`
- real effector/storage boundary: real Tantivy index in RAM built from the real files of the exact index

## Invariants

- The lexical file set equals the exact index's searchable set; generated/ignored/binary paths never enter it.
- Hits are bound to the revision of the document they came from; a removed file leaves the index at refresh.
- Queries are parsed leniently and bounded; malformed queries return no hits, never a panic.

## Verification

- `modbit-retrieval/index` :: `bm25_lexical_index_ranks_refreshes_and_bounds`
- `modbit-core/surface_protocol` :: `m3_2_bm25_lexical_index_ranks_files_and_refreshes_on_writes`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
