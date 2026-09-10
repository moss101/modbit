# Task Card — IMP-EV-0111 Prompt cache key + compaction API awareness

## Identity

- Task ID: IMP-EV-0111
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0111; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0111` — Benchmark reports cached-prefix hit/miss and compaction invalidation correctness.
- Evidence tier: real-system or production-equivalent

## Goal

Track cacheable stable prefix/key economics without copying proprietary algorithm.

## Existing-code audit

- classification: PARTIAL before this batch: `modbit-prompt-compiler` already hashed four stable segments and derived a cache key from the first three, but segment 2 was always the same placeholder and nothing reported hits or misses.
- production entry point: `crates/prompt-compiler/src/lib.rs` (segment hashes, `cache_key`); `ModelInvocationStarted.model_route.cache_key`; `services/modbit-core/src/inspector.rs` `epochs_and_cache`; `ContextInspectorView.prefix_cache_hits/misses`; CLI `context show`; desktop inspector panel.
- proof: The cacheable prefix is the first three prompt segments — system rules, workspace rules and the compaction epoch — so it is stable within an epoch and changes at the boundary, and the pack that changes every turn sits behind it. Every model invocation records the key it routed on, so hits and misses are counted from the log rather than estimated: the inspector reports them and the CLI prints them. On a real run the report is exact — one miss for the first turn and one per epoch, hits on every turn in between — and the test checks the inspector's numbers against the keys in the log.

## Limitations

This is a per-run report from the product, not a standing benchmark suite: nothing yet tracks cache economics across runs or prices them.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `qual_ev_0111_0268_context_show_reports_compaction_epochs_and_the_cached_prefix`
- `stable_segments_share_cache_keys_and_projection_changes_are_hashed`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
