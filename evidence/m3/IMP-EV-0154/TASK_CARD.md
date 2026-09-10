# Task Card — IMP-EV-0154 Lexical + semantic fusion

## Identity

- Task ID: IMP-EV-0154
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0154; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0154` — A/B benchmark vs lexical and semantic-only baselines.
- Evidence tier: real-system or production-equivalent

## Goal

Fuse exact/BM25/semantic with task-aware rerank.

## Existing-code audit

- classification: IMPLEMENTED by M3.7/M3.9 (was NOT-FOUND)
- production entry point: crates/retrieval/src/planner.rs fuse(); bench profiles LexicalOnly, SemanticOnly, BHybrid
- proof: exact, BM25 and semantic candidates are fused by reciprocal rank with task-aware boosts; the harness asserts the hybrid profile's mean recall@5 is never below the lexical-only or semantic-only baseline on the same cases

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
