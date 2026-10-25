# Task Card — PX-103 Hunk attribution to the originating tool call and per-hunk decisions that keep the run going

## Identity

- Task ID: PX-103 (REQ-PX-103; owner workspace-git); qualification QUAL-PX-103 / PX-E2E-103; related REQ-EV-0016, REQ-EV-0067, REQ-EV-0270
- Evidence tier: release-critical (effect-bearing review decisions on a live run; a new attribution record on canonical change evidence)
- Status set by this task: see the handoff. COMPLETE is not claimed: prerequisite PX-061 is REAL_TESTING and the live-gateway leg of PX-E2E-103 has not been run.

## Existing-code audit (production caller to real effector)

- adoption audit: DOCUMENTED-ONLY; no product caller present
- Review projection (`services/modbit-core/src/review.rs::bundle`, `GetReviewBundle`): PRODUCTION-WORKING for hunks, no attribution fields.
- Per-hunk revert (`services/modbit-core/src/changes.rs::revert`, `RevertChanges`, PX-048): PRODUCTION-WORKING through the ChangeTransaction with a checkpoint and file-revision guards, but refused while the loop runs (`require_stopped` → `TASK_RUNNING`) and silent to the agent. IMPLEMENTED-PARTIAL against this task.
- Review decision (`DecideReview`): at the review boundary only; unchanged.
- Attribution of a shell or test write: NOT-FOUND. `services/modbit-core/src/tools.rs` built `FileChanged` events only for `change.apply` and `change.batch` (`change_targets`); the change barrier of FIX-06 (`crates/tools/src/direct.rs::Barrier::settle`) put its snapshot diff in the model-facing structured output only, capped at 12 paths. A hunk written by `shell.exec` (directly or inside `proc.exec`) had no record on the log at all.
- First missing link: the barrier's paths were not on the log, so no join from a hunk to the call that wrote it existed for shell and test writes; and no live per-hunk decision command.

## Implementation

- `crates/tools/src/direct.rs`: the barrier also names (up to 256) the paths it attributes to the call (`workspace_diff.attribution`), excluding the git surfaces and paths put back from the snapshot.
- `services/modbit-core/src/hunks.rs` (new): `barrier_events` (one `BarrierChangeRecorded` per path, beside, never as, `FileChanged`, so undo, the completion gate and verification keep acting only on the file service's records); `attribute` (replays the workspace log over the file from the base text; each line remembers its call; deletions leave markers; a write the log cannot explain is `Unknown`); `decide` (`DecideHunk`: ACCEPT, REJECT, AUTO_ACCEPT); `deliver_notices` (the notice, once, recorded as delivered).
- `services/modbit-core/src/changes.rs`: `revert` becomes `revert_impl` with an optional decision context; a decision is allowed while the loop runs (the file service's content-hash precondition refuses it if the loop wrote the file meanwhile), checks the hunk digest (`STALE_HUNK`), attributes the hunk and writes the `HunkDecided` record in the same transaction as the revert.
- `services/modbit-core/src/review.rs`: `HunkView` carries the attribution, digest, recorded decision and `auto_accept_eligible`.
- `services/modbit-core/src/runtime.rs`: the notice is delivered at the round boundary (beside the background-process wake) and rebuilt from the log on resume.
- `crates/protocol/proto/modbit/v1/surface.proto` (additive `HunkView` 8-17), `changes.proto` (`DecideHunk`, `DecideHunkAck`); generated TS regenerated.
- Docs 20 and 30 updated.

## Verification

- `services/modbit-core/tests/px_103_hunks.rs` (real `modbit-core` process over its socket, real SQLite store, real Git, real child processes via `proc.exec` → `shell.exec`; scripted OpenAI-compatible model; no mock closes a behavior):
  - `qual_px_103_hunks_name_their_call_and_a_mid_run_rejection_reverts_exactly_that_hunk`: one task writes through `change.apply`, a shell command that rewrites a line, a shell redirect and a test runner script; each of the four hunks names the right tool call (compared with the ids on the log) and basis; a decision on a file that moved is `STALE_REVISION` and on a changed hunk `STALE_HUNK`, writing nothing; a reject made while the run is live (held at the model) leaves the file equal by hash to the expected text, the other hunks still attributed, the run not restarted; ACCEPT and AUTO_ACCEPT are recorded; the agent receives the notice once, with the person's reason, at the next boundary and answers it; exactly one `HunkNoticeDelivered`.
  - `qual_px_103_an_unattributable_hunk_is_shown_as_such_and_blocks_auto_accept`: a write by the test process (no tool call) is shown `attributed=false`, basis `UNATTRIBUTED`, not auto-accept-eligible; `AUTO_ACCEPT` is `UNATTRIBUTED_HUNK`; the person can accept or reject it explicitly (the reject notice says the Core could not attribute it).
  - `qual_px_103_sigkill_after_the_revert_recovers_to_one_consistent_outcome`: the Core is killed (SIGKILL of the real process) right after the ack; the restarted Core holds the revert and the decision exactly once, the same command id replays without writing, and the resumed run is told exactly once.
- Regressions run: `px_048_changes`, `px_117_hooks`, `crates/tools` tests, full `surface_protocol` (see the logs in the handoff).
- Mutation check: `mutation-check.txt` (the barrier records are not written: the shell hunks become unattributed and the attribution test fails; restored passes).

## Limitations (unproven gaps)

- The live gateway leg of PX-E2E-103 (real provider) is not run; the model is the repository's scripted OpenAI-compatible server.
- The `SIGKILL` is taken after the ack; the revert and the decision are one transaction, so a kill inside it leaves neither (not injected at an arbitrary instant by an external killer). No separate fault point sits between the revert and the notice delivery, because the notice is derived from the decision record at the next boundary.
- Attribution is by the line state of the replay with a line diff; repeated identical lines can in principle be misaligned and then fail closed to `unattributed` (never to a wrong call); a binary file or a file over 2 MiB written by a process is unattributed.
- Windows and Linux legs are not run by this task; no release gate is attested.
