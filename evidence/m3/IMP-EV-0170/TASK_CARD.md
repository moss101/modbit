# Task Card — IMP-EV-0170 Temporal validity

## Identity

- Task ID: IMP-EV-0170
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0170; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0170` — Stale cache never labeled current.
- Evidence tier: real-system or production-equivalent

## Goal

Evaluate staleness/current worktree/source revision.

## Existing-code audit

- classification: IMPLEMENTED by M3.8 and this batch (was NOT-FOUND)
- production entry point: pack freshness labels (committed / fresh_in_worktree / rehydrated_from_active_revision); ledger use only at the retrieved revision
- proof: a stale index excerpt is never labelled current: the pack re-reads and labels it rehydrated; a ledger record at an older revision does not count as a use

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`
- `ledger_records_injections_and_marks_use_only_at_the_retrieved_revision`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
