# Task Card — IMP-EV-0172 Incremental large-codebase indexing

## Identity

- Task ID: IMP-EV-0172
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0172; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0172` — Cold vs incremental index benchmarks reported.
- Evidence tier: real-system or production-equivalent

## Goal

Update lexical/symbol/semantic/graph indices incrementally.

## Existing-code audit

- classification: IMPLEMENTED by M3.2–M3.6/M3.9 (was NOT-FOUND)
- production entry point: refresh paths of LexicalIndex, SymbolIndex, SemanticIndex and EvidenceGraph; bench cold_index and incremental_ms
- proof: lexical, symbol, semantic and graph indexes update incrementally per changed path; the benchmark report states cold and incremental timings on the fixed corpus

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `m3_5_semantic_chunk_index_serves_nearest_chunks_and_reembeds_only_changed_files`
- `m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
