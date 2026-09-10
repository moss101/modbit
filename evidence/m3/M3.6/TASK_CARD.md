# Task Card — M3.6 dependency/Git/test/runtime evidence graph

## Identity

- Task ID: M3.6
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `graph` module; `search.graph` tool in `crates/tools`; per-workspace graph, Git history and worktree-diff ingestion and write-time refresh in `services/modbit-core/src/tools.rs`; `Repo::log_recent` in `crates/git`)
- Requirement IDs: docs/18 "Index inputs" (import/dependency edges, Git ownership and change history, changed-line context, test mappings, recent runtime evidence), "L2/L3 expansion" (neighbours by import, co-change and test reach), "Index freshness" (changed paths re-enter the graph at the new revision); docs/43 M3.6; REQ-EV-0157 / REQ-EV-0158 groundwork
- Risk class: medium (read-only over the real repository; import resolution is heuristic per language convention and says so)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

An in-memory evidence graph per workspace: import edges from the real tree-sitter parse of every Rust/Python/TypeScript/JavaScript file resolved to workspace paths by language convention (`mod`/`crate::`/own-crate paths, relative and package-style Python modules, relative JS/TS specifiers with index files); co-change counts, owners by commit count and the file's own recent commits from the real `git log` (200 most recent, with touched paths); changed line ranges from the real worktree diff versus HEAD; test mappings from test files whose imports reach the path (to depth 3); and runtime evidence from the verification checks of the task's runs, attributed to the test file whose symbol the check id names. Exposed as `search.graph` with relation filters and an import-expansion depth; every view names the graph revision, the graph refreshes the changed paths and the worktree lines on every write, and rebuilds when the workspace revision moved externally.

## Non-goals

Whole-program semantic resolution (the LSP tools of M3.4 answer references/definitions precisely), persistence across Core restarts, ranking or fusion of graph neighbours with lexical/semantic hits (M3.7), blame at line granularity (co-change and ownership are per commit).

## Existing-code audit

- classification: NOT-FOUND (no dependency graph, history or test-mapping index; `crates/git` had diff but no log helper)
- production entry point: `modbit_retrieval::EvidenceGraph` (build / refresh / set_evidence / query), `graph::import_specifiers`, `graph::resolve_import`, `graph::changed_lines_from_unified`; `modbit_git::Repo::log_recent`; `ToolHost::graph`; `IndexPort` serves kind `graph`
- real effector/storage boundary: real tree-sitter parses, the real `git log`/`git diff` of the workspace, the event store's verification-run projection

## Invariants

- Edges only point at paths that exist in the workspace file set; unresolved specifiers (std, packages) are dropped, never guessed.
- A refresh re-derives the changed path's outgoing edges and removes a deleted path from both directions; the revision moves with the workspace.
- Views are bounded (`max_hits`, default 50) and report the revision they were answered at.

## Verification

- `modbit-retrieval/graph` :: `import_specifiers_come_from_the_real_parse_of_each_language`, `specifiers_resolve_to_workspace_paths_per_language_convention`, `changed_lines_follow_the_new_side_hunks_of_a_unified_diff`, `graph_answers_imports_importers_history_tests_evidence_and_refreshes_per_path`
- `modbit-git/real_git` :: `log_recent_lists_commits_with_their_paths_newest_first`
- `modbit-core/surface_protocol` :: `m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence`, `m2_8_verification_engine_gates_completion_on_real_cargo_fixture` (runtime evidence attributed to `tests/quantities.rs` after the real verification runs)
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
