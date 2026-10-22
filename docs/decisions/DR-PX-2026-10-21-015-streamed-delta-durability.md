---
id: DR-PX-2026-10-21-015
title: Streamed assistant-text deltas commit without a per-delta fsync; every authoritative commit still hardens them
status: accepted
date: 2026-10-21
supersedes: none
approved_by: owner, chat instruction of 2026-10-21 ("Accept both Decision 1 and Decision 2")
---

# DR-PX-2026-10-21-015 — durability of streamed deltas

> **Accepted 2026-10-21** by the owner's chat instruction, Decisions 1 and 2. Decision 3 stays deferred. The measurements below were taken on a throwaway instrumented copy; the tree carries only what section 3 describes.

## 1. Trigger and evidence

QUAL-PX-041 / AFW-K01: a streamed delta renders within 100 ms of its event timestamp at p95. `perf_px_041_delta_latency_from_the_models_piece_to_the_client_is_a_recorded_distribution` asserts the Core's share (event timestamp → a client holding the event) at p95 ≤ 100 ms. It is red on main on the hosted Linux and Windows runners and green on macOS:

| Run | OS | Core share p50 / p95 / p99 / max (ms) | n |
|---|---|---|---|
| 39477754477 (main) | Windows | 26 / 154 / 227 / 278 | 199 |
| CI of #108 | Windows | 22 / 158 / 315 / 384 | 199 |
| 39380048743 (main) | Linux | 2 / 139 / 176 / 293 | 210 |
| local, 6 runs | macOS | 1 / 2 / 2–3 / — | 230 |

The Linux row is bimodal: the median is fine, 5 % or more of deltas are slow. That is the signature of a stall, not of steady cost.

**Where the time goes (measured).** Every delta is one SQLite transaction on `core.db` with `synchronous=FULL` (`crates/event-store/src/store.rs:483`; docs/31 "Local storage"), appended synchronously while the Core's store lock is held (`services/modbit-core/src/stream.rs`, `publish`). Instrumented locally, the lock wait is 0.0 ms at p95 for a lone stream; the cost is the commit. On macOS `fsync` does not flush the drive cache, so FULL costs 0.5 ms and the bench shows no effect at all there; the hosted runners' disks do flush, and the Rust tests of one file run in parallel, each with its own Core, so 10+ Cores fsync the same disk at once.

**Reproduction of the stall without the Core** (`crates/event-store/examples/delta_bench.rs`, the same driver on the real API, the real `EventStore`, 230 single-delta appends 60 ms apart, 16 other writers committing FULL on the same filesystem; Linux VM disk under Docker, 6 runs per variant, `full` against `relaxed`, run as `delta_bench <full|relaxed> 16`):

| Variant | append p50 (median of runs) | append p95 (median / worst of runs) | p99 worst | deltas > 100 ms |
|---|---|---|---|---|
| A. FULL (today) | 3.6 ms | 68.6 / 91.9 ms | 633 ms | 38 of 1380 (2.8 %) |
| B. NORMAL for delta appends | 0.4 ms | 11.6 / 43.8 ms | 391 ms | 11 of 1380 (0.8 %) |
| C. B + no autocheckpoint on the delta path | 0.3 ms | 1.5 / 15.3 ms | 80 ms | 2 of 1380 (0.1 %) |

With no neighbours (0 writers) FULL is 2.3–2.6 ms p95 and NORMAL 0.5–0.8 ms; with 4 neighbours FULL showed a 243–318 ms tail in one of two runs.

**What this does not show.** Not measured: the hosted Windows runner (its p50 of 22–26 ms says its cost is not only the fsync), the Core-level test on CI hardware with any variant, and power loss (no way to induce it here). The bench is a Docker Linux VM disk, not a hosted runner's.

## 2. Current behavior

Each delta (≤ 2 KiB, at most one per 50 ms per stream) is appended in its own `IMMEDIATE` transaction committed under FULL, then published to clients. The aggregate is `AssistantStream`. The authoritative record of the message is the `AssistantMessageCompleted` event, whose `text_ref` names a content-addressed object written with `sync_all` before the event is appended; deltas are the live preview of it (doc 65, PX-041: "a client dropped mid-stream resumes by cursor with no gap and no duplicate"; "`SIGKILL` of the Core mid-stream leaves the stream closed as aborted-by-recovery").

## 3. Proposed replacement

**Decision 1 (this record's core).** Delta appends of the `AssistantStream` aggregate commit with `synchronous=NORMAL`; every other append keeps `synchronous=FULL`. Concretely, `EventStore` gets an explicit durability argument on the append path (an enum `Durability::{Full, Relaxed}`; `Relaxed` is accepted only for events of type `AssistantTextDelta`, enforced by the store like its other stream rules), not a pragma left toggled; `stream.rs::publish` is the only caller that passes `Relaxed`.

Why this is safe, stated as the invariants it must keep:
- *Process crash (SIGKILL of the Core):* a NORMAL commit in WAL mode is written to the WAL file before the call returns, so it survives the death of the process. Only the sync to disk is skipped. (Checked: `qual_px_041_a_core_killed_mid_stream_leaves_an_aborted_stream_after_restart` passes with the relaxed commit.)
- *Power loss or OS crash:* the WAL is a prefix-ordered log and the next FULL commit syncs the whole file, so the only thing that can be lost is the tail of deltas after the last FULL commit, never a delta in the middle and never an authoritative event (the completion record, tool results and receipts are FULL commits and harden everything before them). A lost tail leaves the stream open with fewer deltas; recovery already closes open streams as aborted-by-recovery and the run re-requests under the existing turn semantics. Per-aggregate sequence and the integrity-hash chain stay contiguous because the surviving WAL is a prefix. This relies on SQLite's documented WAL semantics and has **not** been demonstrated here; the proposed test below does.
- *Redaction, ordering, cursor replay, the 2 KiB and 50 ms bounds* are untouched.
- docs/31 says `synchronous=FULL` "for authoritative event/protocol/effect commits"; deltas are the non-authoritative preview of the completion record, and this record is that clarification.

**Decision 2 (test policy, independent of Decision 1).** `perf_px_041_...` asserts the Core's share only on the reference profile, as `apps/desktop/e2e/perf-budgets.spec.ts` already does (`ENFORCE = PROFILE === "reference"`), and records the distribution everywhere else (`MODBIT_PERF_PROFILE`). Today the Rust test enforces on every CI runner and the desktop spec does not; the two rows measure one budget and should follow one rule. This weakens no product guarantee but means CI no longer proves the 100 ms budget on shared runners; the reference-profile run (a named machine) does.

**Decision 3 (deferred, not proposed now).** Variant C — taking the WAL autocheckpoint off the delta path — is the biggest win in the bench but needs a checkpoint owner (a background task, a WAL size bound, and a change that touches every writer). It is recorded as the next step if the reference-profile run of Decision 1 still misses the budget.

**Rejected.** Batching several deltas per transaction (adds latency against a 100 ms budget and the 50 ms window already coalesces; does not remove the stall). Publishing before the commit (breaks "log before publish" and the no-gap/no-duplicate cursor replay after SIGKILL).

## 4. Migration

None. No schema change, no data change, no format change. Existing databases are read exactly as before.

## 5. Compatibility

No protocol change. Clients see the same events in the same order. The difference is on disk only, in the window between a delta's commit and the next FULL commit.

## 6. Security impact

Redaction before append, the secret broker, tenant and path policy are untouched. Nothing credential-bearing is loosened: a delta is redacted model text. The authoritative approval/effect/receipt commits keep FULL, so no protected-effect evidence becomes less durable.

## 7. Test impact

Added with the change (all on a real SQLite file or the real Core):
1. `crates/event-store/tests/relaxed_durability.rs`, `relaxed_is_for_streamed_deltas_only_and_the_connection_is_full_again_after_every_append`: the store refuses `Relaxed` for a completion, for a delta on another aggregate type, for a delta batched with a completion and for an empty batch (`RelaxedRefused`, nothing written), and `PRAGMA synchronous` reads FULL after a success, a refusal and a stream-rule failure. Mutations checked: never returning to FULL, and accepting any event type, each fail it.
2. Same file, `a_lost_wal_tail_costs_only_a_suffix_of_deltas_and_a_full_commit_hardens_the_ones_before_it`: the WAL is cut back to every length it had during a run and to a torn byte past each; the store opens, every aggregate's chain verifies and is contiguous from 1, the deltas present only ever grow with more WAL, a stream is open exactly while it has deltas and no completion, and any cut at or after the completion (a FULL commit) holds all three deltas before it.
3. `services/modbit-core/tests/stream_transcript.rs`, `qual_px_041_a_power_loss_that_takes_the_unsynced_wal_tail_leaves_a_contiguous_prefix_closed_by_recovery`: the real Core is killed mid-stream and its WAL cut to none, half, half plus a torn byte, or all of the tail written since the stream began; the restarted Core shows a contiguous delta prefix, closes the stream aborted-by-recovery counting exactly the surviving deltas (or finds none open), completes nothing and does not resume a loop. The cuts short of all of the tail must lose at least one delta, so the emulation is known to bite.
4. The existing SIGKILL case (`qual_px_041_a_core_killed_mid_stream_...`) stays and passes.
5. `perf_px_041_...` follows Decision 2: the Core-share budget is asserted when `MODBIT_PERF_PROFILE=reference` and otherwise recorded with a verdict (`MET` / `NOT MET`) and the profile; the sample floor is 150 deltas (pieces inside one coalescing window merge on a loaded host).
6. `crates/event-store/examples/delta_bench.rs` keeps the variant table reproducible.

What the emulation is: cutting the WAL file is a model of a power loss at the SQLite level (frames carry a checksum chain, so SQLite recovers to the last whole commit of the prefix it finds). It does not model a disk that persists later blocks without earlier ones; the checksum chain makes SQLite stop at the first invalid frame either way.

## 8. Rollback

One argument at one call site (`publish`): pass `Full` again. No data to migrate back.

## 9. Approval

Accepted on 2026-10-21 by the owner's chat instruction: "Accept both Decision 1 and Decision 2: flip the record to accepted, commit it with the Decision-Record trailer, implement Durability::Relaxed for AssistantTextDelta with the WAL-tail loss tests and the profile rule for perf_px_041, and measure on CI." Decision 3 (taking the WAL autocheckpoint off the delta path) stays deferred.

### Open points (carried, not decided here)
- Windows: Decision 1 is expected to help less there (its p50 is 22–26 ms); the first CI run after the change records the result, and Decision 3 is the fallback if the reference-profile run still misses the budget.
- Decision 2 means CI records, and no longer asserts, the Core's share of the 100 ms budget on shared runners; the reference-profile run is where it is proven.
