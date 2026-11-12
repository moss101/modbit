# PX-041 / PX-047: streamed-delta render latency, before and after (2026-11-07)

Branch `perf/stream-render-latency`, based on `origin/main` at `aa483106`.
This is a measurement record, not a status change: PX-041 and PX-047 stay where
the graph has them. PX-041 still needs its live leg (a real provider through
the gateway), which is blocked on a gateway key, and its acceptance names the
packaged app; these numbers come from the unpackaged build.

## What was measured

- Spec: `apps/desktop/e2e/perf-budgets.spec.ts`, test "PERF PX-041/PX-047: a
  streamed delta renders within 100 ms of its event timestamp at p95, every
  delta and the first of each message".
- Profile `ci` (the spec's default, what CI runs): a debug-built Core
  (`cargo build --workspace --bins`), the unpackaged desktop
  (`pnpm --filter @modbit/desktop build`), budget recorded, not asserted.
- Defaults of the spec: 4 tasks x 50 turns, 4 deltas per message 60 ms apart,
  about 800 deltas per run.
- Machine: Apple M5 Pro, 18 cores, macOS 27.0.1 (Darwin 27.0.0), local, not a
  CI runner. Other projects' jobs were running: load average about 14 for
  baseline 1, about 5 to 6 for baselines 2 and 3 and all three after-runs.
- Retained result files (the spec's own `result.json`, environment included):
  `stream-render-latency-2026-11-07/baseline-{1,2,3}.json` and
  `after-{1,2,3}.json`. Baselines 2 and 3 ran the `origin/main` Core binary
  (`MODBIT_CORE_BIN`) against the same desktop build.

## Results (ms, nearest-rank percentiles, n is deltas)

| run | every delta p50 / p95 / p99 | first delta p95 | to main-process receipt p95 | 4th delta p50 / p95 | verdict |
|---|---|---|---|---|---|
| baseline 1 | 19.7 / 143.6 / 203.9 (n 800) | 117.5 | 135 | 107.6 / 196.9 | NOT MET |
| baseline 2 | 16.1 / 119.6 / 156.3 (n 790) | 99.2 | 109 | 103.5 / 153.2 | NOT MET |
| baseline 3 | 17.0 / 133.5 / 212.1 (n 798) | 110.1 | 121 | 109.7 / 200.4 | NOT MET |
| after 1 | 9.1 / 39.2 / 86.3 (n 795) | 47.1 | 23 | 7.6 / 22.9 | MET |
| after 2 | 8.7 / 38.0 / 97.9 (n 800) | 40.3 | 21 | 7.4 / 27.0 | MET |
| after 3 | 9.1 / 40.5 / 100.4 (n 797) | 41.6 | 25 | 8.0 / 30.9 | MET |

Deltas over 100 ms: 147, 119, 141 before; 6, 8, 8 after. No delta was held up
by a renderer long task in any run, and none went unshown. The window's own
share did not change: main-process receipt to renderer receipt p95 13.5 to
16.3 ms before, 14.1 to 14.9 ms after; renderer receipt to the DOM p95 about
11 ms; DOM to the next frame p95 about 8 to 9 ms.

## Attribution

Temporary instrumentation (removed before the commit; nothing of it is in the
change) logged, per delta, the time the runtime got the store lock and
appended it, when the subscriber's connection began draining it and when it
was written to the socket, plus every command's type and duration on the
connection. The event timestamp (`recordedAt`) is stamped when the runtime
builds the event (`runtime::typed`), before it waits for the store lock.

On one full baseline run (791 deltas, timestamp-to-socket p95 118 ms), the 182
deltas that took over 50 ms to reach the socket lost 19.6 s between them:

1. **7.2 s, 68 deltas: the runtime did not yield after a message's last
   delta.** The append woke the subscriber's connection task, which tokio
   puts in the waking worker's LIFO slot (idle workers do not steal it). The
   runtime then went on, without a single yield, through closing the stream,
   the tool call, the checkpoint and the next turn's preparation (about 95 ms
   on a debug Core), and only then did the connection run. No command was on
   the connection and no lock was held against it. This is the 4th delta of
   every message (p50 107 ms before).
2. **7.5 s, 74 deltas: the connection was busy answering a query.**
   `serve_frames` answers a command inline and drains events only between
   commands. The desktop asks `GetContextAccounting` and `GetTaskEconomics`
   (p50 84 and 102 ms each on that run, sent together) after most events of
   the watched task, and `GetTranscript` / `GetAgentHeaders` (p50 21 and
   13 ms) after every non-delta event.
3. **4.7 s, 65 deltas: the runtime waited for the store lock before the
   append.** `GetTaskEconomics` and `GetContextAccounting` read the whole
   session (every task's events, deltas included) and fold it while holding
   the store mutex.

The drain's own wait for the store lock was negligible (p95 1 ms), and the
desktop's main process and window added p95 about 15 ms.

## The change

1. `services/modbit-core/src/stream.rs` (the stream lane, owner of the
   deltas): after a delta is appended and the lane updated, the runtime
   yields once (`tokio::task::yield_now`), so the woken deliveries run before
   the rest of the turn does. Cause 1.
2. `services/modbit-core/src/server.rs` (`serve_frames`, the connection's
   owner): five read-only queries (`GetTranscript`, `GetAgentHeaders`,
   `GetTaskEconomics`, `GetContextAccounting`, `GetContextInspector`) run
   while the same connection keeps draining the subscription
   (`delivering_events`). The store lock is taken in the same `select!` that
   drives the query, so a query holding the store across an await is never
   waited on by the delivery that paused it. The store is released before
   any frame is written, and frames are written only between polls of the
   query, so no frame is interleaved. Cause 2.

Why the Core-side option and not a dedicated desktop subscription connection:
it keeps the one-connection contract every client relies on (the desktop, the
IDE adapter and the CLI all benefit without a second handshake, a second
reconnect and resubscribe path, or a cursor shared between two sockets), and
it changes ordering only where nothing can depend on it. A command that
changes anything is answered exactly as before, its ack first and then the
events it caused. Only the five queries, which append nothing, may now see an
event delivered before their ack, as if the query had been read a moment
later. Events on the connection stay in offset order with no gap and no
duplicate; the desktop's readers already re-read when an event lands during a
read in flight.

Nothing else moved: the event store, the subscription cursor, replay, lease
fencing, relaxed durability for deltas and crash recovery are untouched. No
second event store, protocol-state store or scheduler was added.

After the change, on an instrumented run (785 deltas), 18 deltas took over
50 ms, 1.4 s between them: 0.9 s waiting for the store lock held by the
economics and accounting folds (cause 3) and 0.5 s of the drain waiting for
the same lock while one of those queries held it. None was left to cause 1.

## Tests

- New: `services/modbit-core/tests/stream_transcript.rs`
  `qual_px_041_events_keep_their_order_while_read_only_queries_run_on_the_same_connection`.
  One raw connection subscribes and asks the four queries back to back while a
  400-word answer streams. Every query is answered with its own ack, the live
  events equal the log's cursor replay (in order, no gap, no duplicate), and the
  deltas make up the completed message. It guards ordering and completeness.
  The latency itself is shown by the perf spec, not by this test.
- Suites run, and the PR body lists them with their results: the
  `stream_transcript` and `surface_protocol` suites, the other Core suites that
  subscribe, clippy `-D warnings`, `cargo fmt --check`, `pnpm typecheck`,
  `pnpm test` and the full desktop e2e suite.

## Limits

- Local macOS numbers on a shared, loaded developer machine with a debug Core
  and the unpackaged app. They are not the packaged CI numbers (PR #179
  measured p95 111.2 ms on the packaged build in CI, before this change), and
  they are not the `reference` profile (release Core, quiet machine), which is
  the only profile that asserts the budget. The `reference` profile was not run.
- Cause 3 is not fixed. The economics and accounting folds still hold the store
  mutex while they read the whole session. They make the tail: p99 86 to
  100 ms after the change, and deltas still over 100 ms at 6 to 8 per run. The
  next step, if the tail matters: fold those views from a read-only SQLite
  connection owned by `crates/event-store` (WAL allows concurrent readers), or
  have them read only the task's rows instead of the session's.
- The live leg of PX-041 (a real provider through the gateway) is not covered
  here; it is blocked on a gateway key.
