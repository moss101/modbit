# Task Card — M4.2 Compaction epochs + async worker + stale rejection + sync fallback

## Identity

- Task ID: M4.2
- Milestone: M4 Durable recovery spine (P0)
- Requirements served: REQ-EV-0056 / 0057 / 0058 / 0092 / 0130 (compaction epochs, M3) extended by docs/19 "Compaction epochs" (session/branch generation captured per request; asynchronous result accepted only while the source branch/generation is current; fork/revert/cancel invalidates pending compactions; bounded synchronous compaction under hard pressure) and docs/31 `compaction_epochs`.
- Qualification: docs/51 `E2E-006` shape and docs/54 fault 10 — an asynchronous compaction whose result returns after the history moved on is rejected and logged, and the new context never includes the stale compacted result.
- Evidence tier: real-system (real Core process over the real socket; the worker delay is the fault-10 injection hook)

## Goal

Move compaction off the agent loop into a worker whose result installs only at a turn boundary and only while the branch and the exact prefix it summarised are current; keep a bounded synchronous compaction for hard pressure; log every request, commit and rejection so the `compaction_epochs` projection is derivable from the log; give the session the branch generation a fork or revert will move.

## Existing-code audit

- classification: PARTIAL before this task. M3 delivered `crates/compaction` (`compact`, `accept`, `fidelity`) and a synchronous compaction at turn start with an `accept` guard keyed on the task aggregate generation and the store head — both move every turn, so an asynchronous result could never have installed; there was no worker, no soft-pressure trigger, no session/branch generation, no logged rejection (`eprintln` only), no `compaction_epochs` projection and no record of the mode. Fork and revert do not exist yet (IMP-EV-0077/0122/0123 later in M4).
- production entry points:
  - `crates/compaction/src/lib.rs` — the manifest carries `branch_generation` and `source_digest` (sha256 over the source entries in order); `accept_async` judges a worker's result by the branch generation it captured, the epoch order and the prefix digest — never by the log head, which has moved on by the time a worker returns; `RejectedCompaction::{BranchChanged, SourceRewritten}` and `code()`.
  - `crates/domain/src/session.rs` — `Session.branch_generation` and `SessionEvent::SessionBranched { branch_generation, kind, reason }` (forward only; a stale generation is refused at the store).
  - `crates/domain/src/task.rs` — `TaskEvent::CompactionStarted` (what the request captured, logged before a worker exists), `TaskEvent::CompactionCommitted` (the durability record beside the model-facing `ContextEpochOpened`, which is bound to its `compaction_id`, `branch_generation`, `mode` and `source_digest`), and `TaskEvent::CompactionRejectedStale` (reason, detail, the refused manifest hash) — the docs/30 "Durability" vocabulary.
  - `crates/event-store` — migration V11: `sessions.branch_generation` and the docs/31 `compaction_epochs` table, projected from the three task events (PENDING → COMMITTED | REJECTED), cleared and rebuilt on `rebuild_projections`; `compaction_epochs(task)`.
  - `services/modbit-core/src/runtime.rs` — `compaction_step` at every turn boundary: harvest a finished worker (`accept_async` → install as `ASYNC`, or `CompactionRejectedStale`); under hard pressure the bounded synchronous compaction (`SYNC_FALLBACK`), request logged first; under soft pressure (¾ of the budget) with no worker in flight, `CompactionStarted` then a `spawn_blocking` worker on a snapshot of the prefix. A worker still in flight when the run ends is stopped and its request closed `RUN_ENDED`; a request an earlier process left pending is closed when the task resumes. `MODBIT_COMPACTION_WORKER_DELAY_MS` is the docs/54 fault-10 hook.
  - `services/modbit-core/src/inspector.rs`, `surface.proto` — `ContextInspectorView.compactions` lists every request with its status, mode, source range and manifest or rejection.
- proof: with a budget that leaves a soft window wider than one turn's growth, a real run starts a worker at soft pressure and installs its manifest at a later boundary as an `ASYNC` epoch bound to the logged request (same id, mode, epoch, branch generation and source digest), and the projection reaches the model at the next request. With the fault-10 hook holding the worker's result for 1.5 s and the model paced at 100 ms per turn, hard pressure comes first and the bounded synchronous compaction installs the epoch (`SYNC_FALLBACK`); the worker's result then returns for a history that moved on and is refused on the log (`NOT_SUCCESSOR` / `SOURCE_REWRITTEN`) with its manifest hash, which never becomes an epoch and never appears in any prompt; every request ends committed or rejected; the inspector's `compaction_epochs` rows agree with the log; the installed epochs form one successor chain. At the store, the branch generation moves forward only, a stale `SessionBranched` is refused, and the value survives a rebuild.

## Limitations

- Fork and revert do not exist yet, so the `BRANCH_CHANGED` rejection is proven at the crate and store level (`accept_async`, `SessionBranched`), and the full E2E-006 with a real fork/revert is owed by IMP-EV-0077/0122/0123 when they land in M4; the runtime path they will exercise is the same `accept_async` harvest.
- The worker delay is a fault-injection hook (docs/54 fault 10) read from the environment; it is unset in production and does nothing when unset.
- Cancelling a run stops the worker and closes its request as `RUN_ENDED`; a cancel does not bump the branch generation, because nothing has rewritten the history.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m4_2_e2e_006_async_compaction_installs_at_a_boundary_and_a_late_result_is_refused`
- `modbit-compaction`: `an_asynchronous_result_installs_only_onto_the_prefix_and_branch_it_saw`, `a_stale_or_out_of_order_compaction_result_is_refused`
- `modbit-event-store`: `session_branch_generation_moves_forward_only_and_is_projected`, `migrates_the_committed_m1_1_fixture_and_derives_projections` (V11)
- Regression: `qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
