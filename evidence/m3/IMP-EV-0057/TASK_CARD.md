# Task Card — IMP-EV-0057 CompactionManifest + hot/warm/cold context

## Identity

- Task ID: IMP-EV-0057
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0057; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0057` — Compaction fidelity test checks labeled instructions/decisions/approvals survive.
- Evidence tier: real-system or production-equivalent

## Goal

Track source head, preserved facts/IDs, resources and compressed projection.

## Existing-code audit

- classification: NOT-FOUND before this batch.
- production entry point: `crates/compaction/src/lib.rs` `CompactionManifest`; the object is stored by the runtime and named by `ContextEpochOpened.manifest_ref`; readable with `ReadObject` and shown by `context show` / the desktop inspector.
- proof: Every epoch writes a manifest that records the source head offset it saw, the number of entries it summarised, the task generation and compiler version it was computed under, the facts it kept verbatim with their kind and origin, the handles that recover the dropped material, the projection and its token cost, and a hash over those fields. The three tiers are explicit: the freshest entries stay verbatim (hot), the labelled facts stay in the projection (warm), and everything else is reachable through the handles and the canonical log (cold). Fidelity is measured, not asserted by eye: `fidelity()` re-derives the critical set from the source and scores the manifest against it.

## Limitations

`fidelity()` shares `compact()`'s labelling heuristic, so it measures whether labelled facts survived compaction, not whether the labels catch everything a reader would call critical.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable`
- `a_critical_fact_corpus_meets_the_fidelity_threshold`
- `a_later_epoch_carries_the_facts_and_handles_of_the_one_before_it`
- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
