# Task Card — IMP-EV-0249 Hybrid exact/BM25/vector retrieval baseline

## Identity

- Task ID: IMP-EV-0249
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0249; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0249` — Run same frozen retrieval benchmark profile with and without Modbit retrieval.
- Evidence tier: real-system or production-equivalent

## Goal

Use as external baseline class, enhanced with structural signals.

## Existing-code audit

- classification: IMPLEMENTED by M3.9 (was NOT-FOUND)
- production entry point: bench profiles ABaseline / BHybrid / CStructural / LexicalOnly / SemanticOnly
- proof: the same frozen cases run with and without the structural engine (profile B hybrid baseline versus profile C) on the same corpus revision; the report states both

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
