# Task Card — M3.1 exact/regex/path index

## Identity

- Task ID: M3.1
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `index` module; search port and `search.*` tools in `crates/tools`; per-workspace index and freshness in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Index inputs" (file paths and exact text), "Concrete local indexing stack" (exact/regex: ripgrep-compatible Rust search path), "Index freshness" (small edits update the exact index incrementally); docs/43 M3.1; REQ-EV-0004 / REQ-EV-0159 groundwork (incremental indexing, freshness)
- Risk class: medium (untrusted repository content; regex from the model)
- Evidence tier: release-critical (Alpha language baseline depends on exact retrieval)
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

The L0 layer of retrieval: an index of every indexable file of a workspace at one revision (git-aware file set: tracked plus untracked-not-ignored; generated/vendor/`.modbit` directories, binaries and files over 2 MiB excluded from text search), exact (`-F`) and regex (Rust `regex`, size-limited) search with path/line/column/byte-span hits bound to the index revision and the file's content hash, path search by glob, bounded hits per file and in total, and incremental refresh of exactly the paths a write changed at the new workspace revision. Exposed to the model as `search.exact`, `search.regex`, `search.paths` (ReadOnly, `fs.read`), compiled into the task's tool surface.

## Non-goals

BM25 (M3.2), AST/symbol index (M3.3), LSP (M3.4), embeddings (M3.5), the retrieval planner and fusion (M3.7), Context Pack packing and the provenance ledger (M3.8), Merkle-incremental hashing (IMP-EV-0004).

## Existing-code audit

- classification: NOT-FOUND (`fs.glob` walked the workspace per call; no text search existed)
- production entry point: `modbit_retrieval::RepositoryIndex` (build / refresh / rebuild / search_exact / search_regex / find_paths); `ToolHost::index` and the `IndexPort` search port; `change.apply` / `change.batch` refresh the changed paths
- real effector/storage boundary: real files on disk read through the real `git ls-files -co --exclude-standard`; the index is in-memory per Core process and rebuilt when the workspace revision moves without a recorded change set

## Invariants

- Hits are never served from a revision newer than the index; every hit carries `index_revision` and the file `content_hash`.
- Generated, vendored, ignored and `.modbit*` paths never enter the index; binaries and oversized files are recorded but not searchable.
- Regexes are size-limited and a bad pattern is a typed failure (`BAD_REGEX`), never a panic or an unbounded run.
- Results are bounded (`max_hits`, per-file cap, line bytes) and declare truncation.

## Verification

- `modbit-retrieval/index` :: `exact_regex_and_path_hits_are_bounded_and_revision_bound`, `refresh_reindexes_exactly_the_changed_paths_at_the_new_revision`, `oversized_files_are_recorded_but_not_searched_and_non_git_roots_walk`
- `modbit-core/surface_protocol` :: `m3_1_exact_regex_path_index_serves_bounded_revision_bound_hits_and_refreshes_on_writes`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test` (the three search tools are in the matrix)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
