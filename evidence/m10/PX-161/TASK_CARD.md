# PX-161 task card — read-only tool calls of one turn run concurrently

Decision Record: DR-PX-2026-10-26-018. Evidence tier: release-critical (execution and persistence behaviour change).

## Audit (before)
Classification: DOCUMENTED-ONLY. `services/modbit-core/src/runtime.rs` ran the calls of a turn strictly in sequence (`for (call_id, name, arguments_json) in calls`), although providers declare parallel capability. First missing link: a dispatch step that starts a run of read-only calls together.

## Change
- `runtime.rs`: `parallel_read_only` (a call qualifies only if its tool is in the `fs.`, `git.`, `search.` or `knowledge.` family and the registry's `effect_in_profile` for its own arguments is `ReadOnly`), `parallel_run` (the unbroken stretch of such calls, at most `PARALLEL_READ_BOUND` = 8), and in the action loop: when a qualifying call reaches dispatch, its run is started as one `tokio::spawn`ed task per call; each call then takes its own result from the batch at its place in the sequence, so the harness checks, step records, transcript entries and `apply_entry` stay in the order the model issued the calls. A write, a process, an approval-bearing call or anything not read-only is not in a run and executes alone, after every earlier call has its result.
- `crates/tools/src/direct.rs`: `fs.read` released the workspace lock after the read instead of holding it through shaping the result.

## Proof
`services/modbit-core/tests/px_161_parallel_reads.rs` (real Core process, real git repository, real SQLite store, scripted model): six `git.diff` calls and a `change.apply` in one turn.
1. Overlap and order: the batch takes less wall time than the sum of its calls' own times (span x10 < own x8); the write is proposed only after every read has its result; the model's next request carries the results as call_1_0..call_1_6 in call order; the batch turn's TOOL_CALL steps have rising ordinals.
2. A failing read (`fs.read` of an absent file in the middle): its error is its result, the other five results are present, the write ran.
3. SIGKILL once three calls of the batch are dispatched: after a restart and StartTask no call has two recorded results, and the model hears the batch once, in order.
Mutation: `evidence/m10/PX-161/mutation-check.txt` (no call qualifies → the overlap test fails).

## Known limits
- The log of each call's own lifecycle events (ToolCallProposed/Dispatched/Succeeded) is appended as the call proceeds, so those events of different calls of a batch interleave in completion order; the order the model, the transcript and the step records see is the call order.
- Concurrency is bounded by what the tool itself serializes. The workspace service is one async mutex, so `fs.*` file tools overlap only in decoding and shaping; git and retrieval tools overlap fully. The overlap test uses `git.diff` for that reason.
- A call of a run that the harness then refuses at its turn (plan, scope, retrieval rules) has already executed as a read; its result is discarded and the refusal recorded as before. Reads are side-effect free, and the discarded call remains on the tool-call log.
