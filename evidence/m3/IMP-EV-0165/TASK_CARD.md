# Task Card — IMP-EV-0165 Reranking

## Identity

- Task ID: IMP-EV-0165
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0165; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0165` — Rerank improves relevant-file recall without unacceptable latency.
- Evidence tier: real-system or production-equivalent

## Goal

Rank by task intent/revision/proximity/coverage/provenance.

## Existing-code audit

- classification: IMPLEMENTED by M3.7/M3.9 (was NOT-FOUND)
- production entry point: planner fuse() boosts: exact_symbol, exact_path, fresh_in_worktree, changed_lines, diagnostic, dependency_distance
- proof: reranking by intent, revision freshness, proximity and provenance raises relevant-file recall from 0.81 (baseline) to 0.94 (hybrid/structural) on the fixed cases with ~10 ms mean latency (report.json)

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `l0_serves_a_known_identifier_without_escalating_and_boosts_the_symbol`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
