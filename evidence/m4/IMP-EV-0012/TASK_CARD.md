# Task Card — IMP-EV-0012 Checkpoint epoch fencing

## Identity

- Task ID: IMP-EV-0012
- Milestone: M4 Durable recovery spine (P0)
- Requirement: REQ-EV-0012; owner label: Reliability Layer; subsystem: durability
- Qualification: `QUAL-EV-0012` — Race two checkpoint writes; old epoch cannot overwrite newer state.
- Evidence tier: real-system (the real Core, real worktree, hard kill and restart)

## Goal

Monotonic checkpoint epoch rejects stale asynchronous writers.

## Existing-code audit

- classification: IMPLEMENTED by M4.3 (this card ladders the requirement on M4.3's evidence).
- production entry points: `crates/checkpoint` (`CheckpointManifest` baseline/delta with `DELETED` sentinel, `accept_commit` strictly-newer + integrity hash, `chain_to` / `materialize` / `validate`); event-store V12 `checkpoints` projection that refuses a non-newer `CheckpointCommitted`; `services/modbit-core/src/checkpoint.rs` capture/restore; `CreateCheckpoint` / `ListCheckpoints` / `RestoreCheckpoint` on the wire; `before_completion` and `before_revert` hooks; fault-9 hook `MODBIT_FAULT_CHECKPOINT_DELAY`.
- proof: see `evidence/m4/M4.3/TASK_CARD.md`.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0012_0013_e2e_007_checkpoint_epochs_are_fenced_and_restore_validates_every_object`
- Unit: `baseline_then_deltas_materialize_to_the_final_state_and_validate_every_object`, `a_stale_epoch_never_commits_over_a_newer_one_and_tampering_is_caught`, `broken_chains_are_refused`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names); the M4.3 run json copy alongside
