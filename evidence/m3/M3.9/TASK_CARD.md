# Task Card — M3.9 retrieval benchmark harness

## Identity

- Task ID: M3.9
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `bench` module; case set `tests/fixtures/retrieval-bench/cases.json`; harness test `crates/retrieval/tests/bench.rs`; report copy in this directory)
- Requirement IDs: docs/18 "Benchmark gates inherited from project research" (same-corpus, same-cases comparison of profiles A baseline exact/search tools, B hybrid exact + BM25 + vector, C Modbit structural context engine; repository-engineering metrics relevant-file recall@K, evidence precision, changed-code impact accuracy, cold-index time, incremental-index latency; the token/tool-call/agent-time reductions are targets, not claimed results); docs/43 M3.9; QUAL-EV-0153 / QUAL-EV-0154 groundwork (recall@K / A-B benchmark instruments)
- Risk class: low (measurement only; no production behavior beyond the planner's level ceiling)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

`modbit_retrieval::bench::run(root, corpus, cases, k)` builds every index over a real Git worktree (timing each cold build), measures the incremental refresh of one real file through all indexes, then runs every case under the three profiles through the M3.7 planner — A capped at L0 with `exact` intent, B capped at L1 with `hybrid` intent, C the unrestricted planner — and reports per case the ranked top-K paths, recall@K, precision@K, impact accuracy (impacted paths found), steps executed, the level reached and latency, plus per-profile means and full-recall counts. The report names the harness version, corpus, file count and bytes, and states its method, including that the docs/18 reduction targets are not claimed. The harness test copies the repository's `crates/`, `services/`, `apps/cli/` and `docs/` into a temporary worktree (252 files, 2.8 MB at the benchmarked commit) and runs the eight seeded cases; `MODBIT_BENCH_REPORT=<path>` writes the JSON report.

## Measured at the sealed commit (report.json here; macOS dev machine, debug build)

- Profile A (L0): mean recall@5 0.81, precision 0.41, 6/8 full-recall cases, ~2.1 steps.
- Profile B (≤L1): mean recall@5 0.94, precision 0.29, 7/8 full-recall, ~4.9 steps.
- Profile C (L0–L3): mean recall@5 0.94, precision 0.29, impact accuracy 1.0 on the two impact cases, 7/8 full-recall, ~9.4 steps.
- Cold index ~2.1 s total (semantic 0.7 s, symbols 0.5 s, graph 0.5 s, BM25 0.3 s, exact 0.03 s); incremental refresh of `services/modbit-core/src/server.rs` ~90 ms.
- The harness surfaced a real defect before sealing: graph-expansion neighbours outranked the seeds and dropped a relevant test file out of the top K (profile C recall 0.875 < B 0.94, impact 0.5). Fix: expansion candidates rank below the primary lists and the dependency-distance boost was reduced; C now matches B on recall and reaches impact accuracy 1.0. The one partial case (`l2-importers-of-graph`, recall 0.5 for every profile) is a documented limit: a qualified path (`modbit_retrieval::EvidenceGraph` without a `use`) is not an import edge.

## Non-goals

Same-model, same-task agent runs measuring input-token, tool-call and agent-time reductions (needs a live provider; DR-M2-001), SWE-QA-style multi-hop questions (P1), a learned embedder in profile B (the hashing embedder is what ships).

## Existing-code audit

- classification: NOT-FOUND (no benchmark instrumentation)
- production entry point: `modbit_retrieval::bench::{run, Case, Profile, Report}`; `PlanRequest::max_level`
- real effector/storage boundary: real indexes over a real Git worktree; wall-clock timings

## Invariants

- Every profile sees the same corpus, cases and K; profile ceilings hold (A ends at L0, B at or below L1).
- Metrics are computed from the returned paths against the case's ground truth, never from the planner's own claims.
- The report never states an unmeasured number as a result.

## Verification

- `modbit-retrieval/bench` :: `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `modbit-retrieval/planner` :: `structural_and_engineering_intents_expand_through_the_graph_with_distance_and_diagnostic_boosts` (ranking after the fix)
- `modbit-core/surface_protocol` :: `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- `report.json` — the harness output at the sealed commit (`MODBIT_BENCH_REPORT=… cargo test -p modbit-retrieval --test bench`)
- CI run json/log copies alongside
