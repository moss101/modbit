# Task Card — IMP-EV-0058 Async compaction stale-result guard

## Identity

- Task ID: IMP-EV-0058
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0058; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0058` — Delay compactor while adding events; stale result cannot install.
- Evidence tier: real-system or production-equivalent

## Goal

Reject async compaction if source epoch/head advanced; synchronous bounded fallback under pressure.

## Existing-code audit

- classification: NOT-FOUND before this batch.
- production entry point: `crates/compaction/src/lib.rs` `accept` and `RejectedCompaction`; called in `services/modbit-core/src/runtime.rs` `maybe_compact` immediately before an epoch is appended.
- proof: A compaction result installs only while its source is still current. `accept` refuses three ways: the store head advanced past the range the compaction summarised, the task generation changed (fork, revert or any later task event), or the epoch is not the successor of the installed one. The runtime snapshots the generation and head before compacting, re-reads both after, and refuses rather than installs on a mismatch. The real-system test takes the manifest the run actually installed, shows it was acceptable at the head it saw, and shows the same manifest refused once the log has moved on and refused again when its epoch is already installed. Under pressure the fallback is the synchronous bounded compaction the run loop performs, budgeted by MODBIT_COMPACTION_TOKEN_BUDGET (default 12000 tokens) with a projection target of a quarter of it.

## Limitations

Compaction runs synchronously in the run loop — the bounded fallback docs/19 requires. There is no background compaction worker yet, so in production today the guard fires only on a generation change or a repeated epoch; the head branch is proven against the real store and the real manifest rather than against a live race.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `a_stale_or_out_of_order_compaction_result_is_refused`
- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
