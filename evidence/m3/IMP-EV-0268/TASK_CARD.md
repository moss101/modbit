# Task Card — IMP-EV-0268 Prompt-cache-aware compaction

## Identity

- Task ID: IMP-EV-0268
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0268; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0268` — Benchmark cache hits/misses and verify no stale context after fork/revert.
- Evidence tier: real-system or production-equivalent

## Goal

Compaction and prompt assembly account for stable cacheable prefixes and cache invalidation.

## Existing-code audit

- classification: PARTIAL before this batch (see IMP-EV-0111): the segment layout existed, the epoch that moves it did not.
- production entry point: `crates/prompt-compiler/src/lib.rs` (the epoch segment folded into the stable prefix); `crates/compaction` `accept`; `services/modbit-core/src/runtime.rs`; `ContextInspectorView`.
- proof: Prompt assembly puts the compaction epoch in the stable prefix and the volatile context pack after it, so compaction — the one thing that must invalidate the cache — is exactly what moves the key, and nothing else does. Measured on a real run: the number of distinct prefixes equals the number of epochs plus one, the number of transitions equals the number of epochs, and the prefix never changes inside an epoch. Stale context after a fork or revert is refused by the same guard that governs installation: a manifest computed under a different task generation cannot install.

## Limitations

Cache hits and misses are counted from this product's own log, not measured against a provider's cache accounting; fork and revert commands do not exist yet, so their invalidation is proven through the generation guard rather than through a fork.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `qual_ev_0111_0268_context_show_reports_compaction_epochs_and_the_cached_prefix`
- `stable_segments_share_cache_keys_and_projection_changes_are_hashed`
- `a_stale_or_out_of_order_compaction_result_is_refused`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
