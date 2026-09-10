# Task Card — IMP-EV-0166 Token-budget packing

## Identity

- Task ID: IMP-EV-0166
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0166; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0166` — Budget never exceeded and required critical facts retained.
- Evidence tier: real-system or production-equivalent

## Goal

Pack highest value fragments under explicit budget/coverage priorities.

## Existing-code audit

- classification: IMPLEMENTED by M3.8 (was NOT-FOUND)
- production entry point: crates/context/src/lib.rs pack(); context.pack required_paths
- proof: token_used never exceeds token_budget; required paths and diagnostic-linked entries are packed first; a critical entry that cannot fit marks the pack incomplete instead of being dropped silently

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `budget_is_never_exceeded_critical_entries_go_first_and_omissions_are_summarised`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
