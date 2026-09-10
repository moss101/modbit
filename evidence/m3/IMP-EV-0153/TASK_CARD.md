# Task Card — IMP-EV-0153 Semantic code retrieval

## Identity

- Task ID: IMP-EV-0153
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0153; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0153` — Repo-QA benchmark measures recall@K and precision.
- Evidence tier: real-system or production-equivalent

## Goal

Meaning-based retrieval returns revision/provenance candidates.

## Existing-code audit

- classification: IMPLEMENTED by M3.5/M3.9 (was NOT-FOUND)
- production entry point: crates/retrieval/src/semantic.rs; search.semantic; bench SemanticOnly profile
- proof: semantic hits carry the chunk's path, span, hash and the revision it was embedded at, plus the embedder id and generation; the benchmark measures recall@5 and precision for the semantic-only and hybrid profiles on the fixed corpus

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `semantic_index_finds_nearest_chunks_and_reembeds_only_changed_files`
- `m3_5_semantic_chunk_index_serves_nearest_chunks_and_reembeds_only_changed_files`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
