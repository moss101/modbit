# Task Card — IMP-EV-0130 Conversation compaction

## Identity

- Task ID: IMP-EV-0130
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0130; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0130` — Critical-fact compaction corpus meets fidelity threshold.
- Evidence tier: real-system or production-equivalent

## Goal

ContextEpoch + manifest over lossless durable history.

## Existing-code audit

- classification: NOT-FOUND before this batch.
- production entry point: `crates/compaction/src/lib.rs` (`compact`, `fidelity`, `FactKind`); the run loop's epoch installation.
- proof: Instructions, plan decisions, approvals and open failure signatures are kept verbatim; everything else becomes a counted summary plus the handles that recover it. A later epoch carries the earlier epoch's facts and handles forward, so a second compaction cannot drop what the first one saved. Fidelity is measured on a corpus of twelve labelled facts among sixteen entries: every labelled fact survives into the manifest (1.0) and at least 90% reach the projection under a realistic budget, while a deliberately tight budget shows the manifest still holding what the projection had to omit.

## Limitations

The corpus is a constructed one that exercises each label; there is no recorded transcript corpus from real sessions yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `a_critical_fact_corpus_meets_the_fidelity_threshold`
- `a_later_epoch_carries_the_facts_and_handles_of_the_one_before_it`
- `compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable`
- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
