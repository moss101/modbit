# Task Card — M4.3 Workspace checkpoint baseline/delta objects + epoch fencing

## Identity

- Task ID: M4.3
- Milestone: M4 Durable recovery spine (P0)
- Requirements: REQ-EV-0012 Checkpoint epoch fencing (monotonic checkpoint epoch rejects stale asynchronous writers), REQ-EV-0013 Checkpoint delta journal (checkpoint records runtime/evidence cursor plus worktree deltas instead of transcript snapshots); docs/19 "Checkpoint epochs"; docs/31 `checkpoints`; docs/14 §8.
- Qualification: `QUAL-EV-0012` (race two checkpoint writes; old epoch cannot overwrite newer state), `QUAL-EV-0013` (restore edited worktree and protocol cursor after process restart from baseline+delta), docs/51 `E2E-007` (epochs N and N+1, delayed N commit: N cannot become current after N+1; restore picks N+1 and hash-validates), docs/54 fault 9.
- Evidence tier: real-system (real Core process over the real socket, hard-killed and restarted; the held writer is the fault-9 injection hook)

## Goal

Recoverable worktree and runtime state as a periodic full baseline plus deltas, fenced by a monotonic epoch so a stale writer can never overwrite newer checkpoint state, restored only after every object the chain names has been read back and hash-validated, and taken by the agent loop at the points docs/14 names.

## Existing-code audit

- classification: ABSENT before this task. `crates/checkpoint` was the empty M0.1 scaffold; no checkpoint record, event, table or restore path existed. The revert path was the revision-bound ChangeTransaction history (`undo.rs`) and the Git dirty snapshot (`Repo::snapshot_dirty`, REQ-EV-0022), which docs/14 §8 names as the Alpha stand-in before M4.
- production entry points:
  - `crates/checkpoint/src/lib.rs` — `CheckpointManifest` (epoch, kind, base, path → object hash with a tracked deletion as `DELETED`, Git HEAD, workspace revision, `RuntimeCursor` {store offset, compaction epoch, branch generation, protocol-state digest}, index generation, integrity hash), `capture` (a delta lists only what changed since the base's materialized state), `accept_commit` (strictly newer epoch; intact integrity hash), `chain_to` / `materialize` (baseline then deltas; links, epoch order and every integrity hash checked), `validate` (every object read back and digest-checked before anything is written), `needs_baseline`.
  - `crates/domain/src/task.rs` — `CheckpointStarted`, `CheckpointCommitted`, `CheckpointRejectedStale`, `CheckpointRestored` (docs/30 "Durability").
  - `crates/event-store` — migration V12: the docs/31 `checkpoints` table; the projection refuses a `CheckpointCommitted` whose epoch is not newer than every committed one (the append fails atomically), makes the previous current row `SUPERSEDED`, and records rejections; `checkpoints(task)`.
  - `services/modbit-core/src/checkpoint.rs` — `capture` (claim the epoch on the log first; store the dirty state — tracked changes, untracked files, tracked deletions — as objects; build the manifest; commit fenced twice, by the rule and by the projection; log the loser), `restore` (resolve the chain from the log, materialize, validate every object, then one workspace transaction: checkpoint files from their objects, dirty paths the checkpoint lacks back to HEAD or away, tracked deletions removed; `CheckpointRestored` with the runtime cursor), `MODBIT_FAULT_CHECKPOINT_DELAY` (docs/54 fault 9 hook).
  - `services/modbit-core/src/server.rs`, `surface.proto` — `CreateCheckpoint` (fenced by the session lease), `ListCheckpoints`, `RestoreCheckpoint` (refused while the loop runs).
  - `services/modbit-core/src/runtime.rs` — a `before_completion` checkpoint before the COMPLETION run and a `before_revert` checkpoint before a WORSENED revert (docs/14 §8).
- `crates/event-store` V13 — found by this task's recovery proof and fixed here: a compiled plan id is content-addressed, so two runs that compile the same plan share it, and the V7/V8 routing rows keyed by plan id alone let one run's slots, attempts, admission and activations overwrite another's (EPR-005's "the attempts continue" check had been reading another task's rows). Every routing row is now keyed by `(run_id, plan_id, …)` and the loaders take the run; the tables are recreated and re-derived from the log on upgrade.
- also fixed forward for the M4.2 run (34580712896): `qual_epr_005`'s post-resume assertion accepts both kill points (in flight, or just after the response was recorded, where M4.1 re-enters the call without asking the model again) and checks the pre-kill attempts are kept exactly; the Tier A incremental-index latency test measures forty refreshes per fixture so a runner hiccup does not become the p95.
- proof: two writers race on a real Core: epoch 1 is held between capture and commit while epoch 2 starts and commits; epoch 1's commit is refused (`NOT_NEWER`), recorded as `CheckpointRejectedStale` with the current epoch, and epoch 2 stands. A delta on the baseline records exactly the changed file and the tracked deletion (2 entries, the unchanged file not repeated), with the runtime cursor and an integrity hash that the manifest carries. After a hard kill and restart, a corrupted object refuses the restore (`OBJECT_MISMATCH` naming the path) with the worktree untouched; with the object intact, the restore returns the worktree to the delta's state (edit restored, deletion restored, untracked file back, a later file gone), the runtime cursor with it, and `CheckpointRestored` is on the log; restoring to the superseded baseline by id works too; the table derived by the new process matches the one before the kill. A scripted run reaches ReadyForReview with a `before_completion` baseline whose manifest names the file it wrote.

## Limitations

- The runtime cursor is recorded and returned by restore; re-entering the run at that cursor (rewind of the transcript and the log) is IMP-EV-0123's rewind/session tree, which will consume it. Restore is refused while the loop runs.
- Terminal, browser and sandbox reattachment metadata in the checkpoint arrive with M4.5's cursor interfaces.
- Untracked files are captured as git reports them (respecting the repository's ignore rules); verification residue in an unignored directory is captured like any other untracked file.
- Baselines are taken after every eight deltas; no garbage collection of superseded chains yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0012_0013_e2e_007_checkpoint_epochs_are_fenced_and_restore_validates_every_object`
- `modbit-checkpoint`: `baseline_then_deltas_materialize_to_the_final_state_and_validate_every_object`, `a_stale_epoch_never_commits_over_a_newer_one_and_tampering_is_caught`, `broken_chains_are_refused`
- `modbit-event-store`: `migrates_the_committed_m1_1_fixture_and_derives_projections` (V12)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
