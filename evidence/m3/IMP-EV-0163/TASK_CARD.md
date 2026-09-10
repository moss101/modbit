# Task Card — IMP-EV-0163 Query decomposition

## Identity

- Task ID: IMP-EV-0163
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0163; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0163` — Planner benchmark records subqueries and coverage.
- Evidence tier: real-system or production-equivalent

## Goal

Break broad requests into targeted retrieval operations.

## Existing-code audit

- classification: IMPLEMENTED by M3.7/M3.9 (was NOT-FOUND)
- production entry point: planner identifier_tokens (sub-queries per identifier), PlanResult.steps and coverage_paths; bench steps per case
- proof: a broad request is broken into per-source and per-identifier retrieval steps; the plan records every sub-query with its candidate count and the coverage reached, and the benchmark records the steps per case

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `unknown_wording_starts_hybrid_and_a_miss_escalates_only_while_coverage_is_short`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
