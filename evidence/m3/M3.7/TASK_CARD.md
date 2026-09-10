# Task Card — M3.7 L0–L3 retrieval planner + fusion

## Identity

- Task ID: M3.7
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `planner` module; `search.retrieve` tool in `crates/tools`; planner adapter over the per-workspace indexes and the task's verification evidence in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Retrieval planner" (L0 exact → L1 hybrid → L2 structural → L3 engineering; start at the cheapest level the query's features support; escalate only when coverage is insufficient), "Fusion" (rank-based fusion plus deterministic boosts for exact symbol/path match, workspace freshness, changed lines, diagnostic linkage and dependency distance; duplicates collapsed by span identity; no LLM in the ranking path); docs/43 M3.7; REQ-EV-0154 / REQ-EV-0163 / REQ-EV-0164 / REQ-EV-0165 groundwork
- Risk class: low (read-only composition of the M3.1–M3.6 indexes; deterministic ranking)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

A planner that reads the query's features (path-like, identifier-like, natural language, structural or engineering wording; an explicit `intent` overrides) and starts at the matching level: L0 runs exact text, symbol and path lookups; L1 adds BM25, semantic nearest chunks and an exact overlay of identifier tokens; L2 expands the top seeds through the evidence graph's import/importer edges; L3 adds test reach, co-change and run evidence. A level escalates only while the fused result covers fewer unique paths than `min_paths` (default 3), L2 needs seeds and L3 is reached only by intent. Every candidate list is fused by reciprocal rank (k = 60) with deterministic boosts — `exact_symbol`, `exact_path`, `fresh_in_worktree`, `changed_lines`, `diagnostic` (the failing checks of the task's verification runs, located at the test symbol they name) and `dependency_distance:<d>` — and duplicates collapse on `(path, span)`. The result names the features, the levels started and ended at, every escalation with its reason, every step with its source, query and candidate count, the exact index revision and the embedding generation when L1 ran. Exposed as `search.retrieve`.

## Non-goals

Token-budgeted packing and the provenance ledger (M3.8), the benchmark harness that measures the profiles against each other (M3.9), LLM reranking (docs/18: optional, never the only ranking path), LSP diagnostics as a live boost source (the diagnostic linkage uses the recorded verification evidence; `lsp.diagnostics` stays an explicit tool).

## Existing-code audit

- classification: NOT-FOUND (five independent search tools; no planner, no fusion)
- production entry point: `modbit_retrieval::planner::{features, start_level, retrieve}`, `Sources`, `PlanRequest`, `PlanResult`, `FusedHit`; `IndexPort` serves kind `retrieve`
- real effector/storage boundary: the real M3.1–M3.6 indexes of the workspace and the event store's verification-run projection

## Invariants

- A level below the start never runs unless the start is structural/engineering (which needs seeds from the cheaper levels); a level above the end never runs.
- Fusion is deterministic for a given index state: same candidates, same order.
- Results are bounded by `max_hits` (default 20) and carry the revision/generation they were answered at.

## Verification

- `modbit-retrieval/planner` :: `query_features_pick_the_cheapest_level`, `l0_serves_a_known_identifier_without_escalating_and_boosts_the_symbol`, `unknown_wording_starts_hybrid_and_a_miss_escalates_only_while_coverage_is_short`, `structural_and_engineering_intents_expand_through_the_graph_with_distance_and_diagnostic_boosts`
- `modbit-core/surface_protocol` :: `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
