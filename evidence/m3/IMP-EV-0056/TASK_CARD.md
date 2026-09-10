# Task Card — IMP-EV-0056 ContextEpoch

## Identity

- Task ID: IMP-EV-0056
- Milestone: M3 (compaction epochs batch)
- Requirement: REQ-EV-0056; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0056` — Fork/revert invalidates incompatible compaction output and retains canonical history.
- Evidence tier: real-system or production-equivalent

## Goal

Compaction creates versioned model-visible epoch without changing run truth.

## Existing-code audit

- classification: NOT-FOUND before this batch: nothing compacted the model-visible transcript; a long run grew its prompt until the model refused it.
- production entry point: `crates/compaction` (`compact`, `accept`, `CompactionManifest`); `services/modbit-core/src/runtime.rs` `maybe_compact`/`epoch_of` in the run loop; `ContextEpochOpened` on the task aggregate (`crates/domain/src/task.rs`); `modbit_prompt_compiler::compile` segment 2.
- proof: When the model-visible transcript passes the token budget the run opens an epoch: the older entries are replaced by a projection, the epoch is appended to the canonical log as ContextEpochOpened with the manifest's object ref and hash, and the log itself is untouched — the run's truth is exactly what it was. The epoch number is monotonic, rebuilt from the log at run start, and an epoch that is not the successor of the installed one is refused. The real-system test drives a task whose transcript compacts nine times, checks the epoch payloads and the manifest object, and shows the model saw the projection instead of the entries.

## Limitations

Session fork and revert commands do not exist yet, so the guard's generation branch is proven at the unit level and against the task aggregate generation the runtime supplies (any fork, revert or later task event moves it). What is proven on the real system is the head branch: the manifest the run installed is refused when offered after the log advanced.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`
- `compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable`
- `a_stale_or_out_of_order_compaction_result_is_refused`
- `a_later_epoch_carries_the_facts_and_handles_of_the_one_before_it`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
