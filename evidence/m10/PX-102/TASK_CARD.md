# Task Card — PX-102 Named checkpoints, retention and garbage collection

## Identity

- Task ID: PX-102 (REQ-PX-102; owner durability; DR-PX-2026-10-05-013); qualification QUAL-PX-102 / PX-E2E-102
- Evidence tier: release-critical (deletion of recovery data under a policy; persistence of checkpoint metadata)
- Branch: `px-102-close-out` from main 6c9c1c79, 2026-11-25

## Existing-code audit

- Adoption audit: DOCUMENTED-ONLY; no product caller present.
- Found on main: PRODUCTION-WORKING for the most part. Wave 1 (#63, 5774ec68) shipped it, but the graph stayed NOT_STARTED, held behind PX-061, which was sealed COMPLETE on 2026-11-24.
  - `services/modbit-core/src/checkpoint_gc.rs`: `RunCheckpointGc`, the decision, the three-event collection, the blob sweep.
  - `checkpoint.rs`: `NameCheckpoint`, the gate between restores and the collector, `semantic_protections`.
  - `crates/event-store` projections: `COLLECTED` rows, blob references.
  - `checkpoints.proto`: `NameCheckpoint`, `CheckpointNamed`, `RunCheckpointGc`, `CheckpointRetentionPolicy`, `CheckpointGcReport`.
  - `checkpoints_px.rs`: six `px_102_*` tests.
  - `mutation-check.txt` (2026-10-25): ignoring the fork reference fails the fork-survival test.
- First missing links:
  1. The card's scope rule "drop all of an archived and applied task after a grace period" was NOT-FOUND. The collector never looked at archive or apply state.
  2. The negative proof "a collection while a restore is in flight fails" was proven only by the gate's unit test, not through the real Core.
  3. Doc 19 ("updated by the implementing task") had no PX-102 section.

## Change

- **`checkpoint_gc.rs` `retired_since`.** A task is retired when its latest `ConversationArchived` has no later `ConversationUnarchived` and its result is in the person's checkout: it ran in place, or its latest worktree apply-back is `APPLIED` (PX-066). A worktree result that was only merged or discarded does not count, which is the conservative choice.
- **`decide()`.** Once a retired task has been archived for the policy's age floor, only the hard references keep a checkpoint: fork parents, pre-restore checkpoints, restore-in-doubt targets and their chains. Names, the latest and the recent ones stop keeping anything.
- **Grace period.** The card sets no number. This reuses `min_age_ms` (default 7 days), so the rule needs no wire change.
- **New tests in `checkpoints_px.rs`:**
  - `qual_px_102_an_archived_applied_task_keeps_only_what_a_hard_reference_needs`: under the ordinary policy the named, latest and fork-parent checkpoints are kept while the task is not archived, after it is unarchived, and inside the grace. Past the grace, only the fork parent and its chain are kept. The named and latest checkpoints are removed, a restore of the named one answers `COLLECTED`, and the fork parent restores exactly.
  - `qual_px_102_a_collection_requested_during_a_restore_never_runs_under_it`: a real restore is held after its journal (`MODBIT_FAULT_CHECKPOINT_STALL=AFTER_RESTORE_JOURNAL:6000`) and a collection is requested during it. Both safe outcomes were observed in runs. Under load the gate refuses `RESTORE_IN_FLIGHT` and nothing is recorded. Alone, the Core serves the collection only after the restore. Either way, on the log `CheckpointGcStarted` comes after `CheckpointRestored`, and the restore lands exactly.
- **Doc 19:** "Named checkpoints, retention and collection as built (PX-102)".

## Acceptance, clause by clause

| QUAL-PX-102 / scope | Proof |
|---|---|
| forty turns, forty checkpoints, three named | `px_102_forty_turns_three_named_every_survivor_restores_to_its_recorded_hashes` |
| the collector thins the finished task by the policy; named, fork-parent and latest survive | `px_102_the_collector_thins_a_finished_task_and_never_removes_what_is_referenced`, `px_102_forty_turns_…` |
| every surviving checkpoint restores to its recorded hashes | `px_102_forty_turns_…` |
| `SIGKILL` mid-collection, restart: no broken chain, the next run completes | `px_102_sigkill_during_the_sweep_leaves_no_broken_chain_and_the_next_run_completes` (external kill during a stalled sweep), `px_102_kill_mid_collection_…` |
| a second collector run is refused while the lease is held | `px_102_a_second_collector_and_a_restore_are_refused_while_a_collection_holds_its_lease` (`GC_LEASE_HELD`; a restore during a collection: `GC_IN_FLIGHT`) |
| the report counts equal the filesystem change | `px_102_the_collector_thins_…`, `px_102_the_byte_bound_…` |
| negative: a collector that removes a needed baseline or a forked parent fails | `mutation-check.txt` (fork reference) |
| negative: a name collision, or a restore of a collected checkpoint without a typed error | `NAME_TAKEN`, `COLLECTED` in `px_102_the_collector_thins_…` |
| negative: a collection while a restore is in flight | **new** `qual_px_102_a_collection_requested_during_a_restore_never_runs_under_it`; gate unit test `the_gate_keeps_the_collector_and_restores_apart` |
| scope: drop all of an archived and applied task after a grace period | **new** `qual_px_102_an_archived_applied_task_keeps_only_what_a_hard_reference_needs`; `mutation-check-retired.txt` |

## Verification (local, macOS, 2026-11-25)

See `tests-2026-11-25.log`, `clippy-2026-11-25.log` and `mutation-check-retired.txt` in this folder.

## Findings and interpretations for the owner

- **No `EffectReceipt` for a collection.** The card says "Collection is a receipted effect". As built, a collection is recorded by `CheckpointGcStarted` / `CheckpointCollected` / `CheckpointGcCompleted`, not by an `EffectReceipt`. Receipts bind a tool call and its kernel decision (docs/23), and `RunCheckpointGc` is a person's command with neither. Doc 19 states this.
- **Grace period.** I reused `min_age_ms`, default 7 days. A different number for retired tasks would need a policy field (an additive wire change).
- **A restore held in the stall hook blocks other commands.** While a restore sat in the stall hook, other commands on that Core (even `ListCheckpoints`, which takes only the store lock) usually waited until it finished. The hook sleeps on a runtime thread, so this may be an artefact of the hook. A long real restore was not measured. Worth checking as Core responsiveness work. It does not affect safety: the collector never ran under a restore in any run.

## Remaining

- PX-102 is a Core row. No desktop or CLI surface is part of its acceptance.
- No release gate is attested.

## CI and merge

- PR 257, CI run 45661876176 on head 177d23d3: conclusion success, every job green (`ci-run-45661876176.json`).
- Merged to main as 6211d58e (squash), 2026-11-26.

## Status

E2E_PROVEN → COMPLETE through `tools/graph.py`, with evidence run:gha-45661876176, commit:6211d58e and `ci-run-45661876176.json`. The owner items under "Findings and interpretations" above stay open as notes. They are not acceptance criteria of QUAL-PX-102.
