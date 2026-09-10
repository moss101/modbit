# Task Card — IMP-EV-0092 Transcript compaction with recoverability

## Identity

- Task ID: IMP-EV-0092
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0092; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0092` — Restart after multiple compactions and reconstruct exact task/protocol state.
- Evidence tier: real-system or production-equivalent

## Goal

Compact active context while retaining lossless canonical events/state.

## Existing-code audit

- classification: NOT-FOUND before this batch.
- production entry point: `services/modbit-core/src/runtime.rs` (`maybe_compact`, `epoch_of`, the rebuild arm for `ContextEpochOpened`); the manifest object store; `crates/compaction` `handles_in`.
- proof: Compaction changes what the model sees and nothing else. The canonical events stay in the log in full, the dropped material stays addressable through the object refs the manifest lists, and `artifact.range` reads any of it back — the projection says so in its own text. After nine epochs the Core is killed and restarted: the log replays to the same epochs, the manifest objects are still readable, the last manifest's hash matches the event's, and its projection is the text the model was given before the restart.

## Limitations

The rebuild is proven by replaying the log into a fresh Core and comparing the epoch events and the installed manifest; it does not resume the suspended run and re-issue a model request.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable`
- `qual_ev_0111_0268_context_show_reports_compaction_epochs_and_the_cached_prefix`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
