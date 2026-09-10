# Task Card — IMP-EV-0159 Freshness/relevance

## Identity

- Task ID: IMP-EV-0159
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0159; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0159` — Deprecated doc is downranked after source change.
- Evidence tier: real-system or production-equivalent

## Goal

Rank active/current knowledge and attach revision validity.

## Existing-code audit

- classification: IMPLEMENTED by M3.7/M3.8 (was NOT-FOUND)
- production entry point: planner boosts fresh_in_worktree and changed_lines; pack entries carry workspace_revision and freshness
- proof: a file changed in the worktree is boosted above unchanged material for the same query and every pack entry names its revision and freshness

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `l0_serves_a_known_identifier_without_escalating_and_boosts_the_symbol`
- `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
