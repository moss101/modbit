# Durable State, Memory, Compaction, and Checkpoints

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Seven-layer durability invariant

1. **Canonical Event / Session Store** — authoritative state transitions.
2. **Protocol State** — tool calls, approvals, questions, terminal/browser/control/subagent lifecycle needed for exact resume.
3. **Context Projection** — bounded model-visible working state for each turn.
4. **Compaction Epochs** — versioned compressed conversation/context history.
5. **Workspace Checkpoints** — recoverable files/worktrees/runtime metadata using baseline + deltas.
6. **Engineering / Semantic Memory** — durable learned knowledge.
7. **Evidence Archive / Effect Ledger** — immutable proof and side-effect provenance.

No layer may silently substitute for another.

## Local persistence

SQLite WAL databases are used for event/projection/protocol/memory metadata with strict migrations and foreign keys. Large immutable payloads, terminal output, browser captures and checkpoint blobs use a content-addressed object store on disk. Core commits event + critical projection changes in one transaction.

## Compaction epochs

Each compaction request captures:
- session/branch generation;
- source event range;
- previous epoch ID;
- prompt/compiler version;
- target token budget.

Async compaction result is accepted only if the source branch/generation is still current. Fork/revert/cancel invalidates incompatible pending compactions. If context reaches hard pressure before async result arrives, Core executes bounded synchronous compaction. The compacted text is context material, **not semantic memory**.

### Compaction epochs as built (M4.2)

The session carries a `branch_generation` (`SessionBranched`; a fork or revert moves it forward only). At each turn boundary the runtime: harvests a finished worker and installs its manifest only if `accept_async` holds — the branch generation it captured is current, the transcript prefix it summarised still has the same `source_digest`, and its epoch is the successor of the installed one — otherwise it appends `CompactionRejectedStale` with the reason and the stale projection never enters the context; under hard pressure (transcript over `MODBIT_COMPACTION_TOKEN_BUDGET`) it runs the bounded synchronous compaction now (`mode` `SYNC_FALLBACK`); under soft pressure (three quarters of the budget) with no worker in flight it logs `CompactionStarted` and starts a worker off the loop (`mode` `ASYNC`). Every request ends on the log — committed by `CompactionCommitted` beside the `ContextEpochOpened` that names it, or rejected — and doc 31's `compaction_epochs` is projected from those events (doc 30 "Durability": `CompactionStarted`, `CompactionCommitted`, `CompactionRejectedStale`); the Context Inspector lists them. A request left pending by a run or process that ended is closed `RUN_ENDED` when the task resumes. `MODBIT_COMPACTION_WORKER_DELAY_MS` is the docs/54 fault-10 hook that holds a worker's result so a test can move the history first.

## Checkpoint epochs

Checkpoint metadata includes monotonic epoch, base revision, delta object refs, Git HEAD/worktree state, index generation, terminal/browser/sandbox reattachment metadata and integrity hash. A stale epoch can never overwrite newer checkpoint state.

Use periodic full baseline + intermediate deltas. Restore validates every object hash before making the checkpoint current.

### Checkpoints as built (M4.3)

`crates/checkpoint` owns the manifest (`CheckpointManifest`: epoch, kind, base, path → object hash with a tracked deletion as an empty hash, Git HEAD, workspace revision, the runtime cursor, an integrity hash), the fence (`accept_commit`: strictly newer epoch, intact integrity hash), the chain (`chain_to`, `materialize`: baseline then deltas, links and epoch order checked) and the validation (`validate`: every object read back and digest-checked before a byte is written). The Core captures the worktree's dirty state into content-addressed objects, claims the epoch on the log (`CheckpointStarted`) before capturing, commits (`CheckpointCommitted`) only while newer than the current checkpoint — the projection refuses anything else — and records a loser as `CheckpointRejectedStale` (docs/54 fault 9; `MODBIT_FAULT_CHECKPOINT_DELAY=<epoch>:<ms>` is the injection hook). A baseline is taken first and after every eight deltas. Restore (`RestoreCheckpoint`) writes the validated state in one workspace transaction — files the checkpoint has from their objects, dirty paths it does not have back to HEAD or away — records `CheckpointRestored` with the runtime cursor, and refuses the whole restore on the first object that does not match. The agent loop checkpoints before its COMPLETION run and before reverting a WORSENED attempt (doc 14 §8).

## Protocol state examples

- outstanding ToolCall and unknown outcome reconciliation;
- pending Approval with bound intent hash;
- pending user question;
- subagent admission/running state;
- terminal session ID + last acknowledged output cursor;
- browser session/control lease;
- sandbox lease + generation;
- model stream attempt and safe resume boundary;
- active ConditionalExecutionPlan slot activation, budget reservation and the `review_isolated` worktree/process lease of an in-flight review leg.

### Protocol state as built (M4.1)

`crates/protocol-state` is the typed reconstruction: `ProtocolState { calls, approvals, question, leases, reconciled }` with each outstanding call in a phase (`PROPOSED`, `AWAITING_APPROVAL { approval_id }`, `IN_FLIGHT`, `UNKNOWN_OUTCOME { reason }`) and a `ResumeBoundary` derived from it — `RECONCILING` before `AWAITING_APPROVAL` before `AWAITING_ANSWER` before `EXECUTING` before `TURN_START`. The store materializes it under `protocol_state` key `task:<task_id>` (doc 31) in the same transaction as the event that changed it; `GetProtocolState` shows it over the wire with a digest.

Write-ahead: a tool call is `ToolCallProposed` (bound to run, turn, the model's call id and an `arguments_ref`) before validation, and `Validated`/`PolicyDecision`/`Dispatched` are appended by the pipeline's dispatch journal before the effector runs; a dispatch that is not on the log does not run (`JOURNAL_FAILED`).

Restart: a call the dead Core had dispatched becomes `ToolCallUnknownOutcome` (read-only calls are cancelled); the task keeps the wait reason its boundary names and is marked for attention with that boundary. Resume: the run re-enters the outstanding calls of its last assistant message by their recorded ids (`ProtocolStateResumed`), so an approval-gated call finds the same `ApprovalId` bound to the same intent and approving once yields one effect; a call of unknown outcome is reconciled — receipt lookup for protected/external/destructive effects, target inspection for reversible writes — and the observation goes to the model as the call's result (`ToolCallReconciled`); nothing is replayed by the Core.

### Cursor metadata interfaces (M4.5)

`ProtocolState` carries `terminals: Vec<TerminalCursor>` (handle id, request id, argv, replay generation, last acknowledged cursor, running, OutputRef and exit once known), built from `TerminalCreated` / `TerminalOutputAdvanced` / `ProcessExited` in the same materialization as the rest of the state, and the typed interfaces the later milestones record into: `BrowserCursor` (session id, control lease generation, state cursor; M7) and `SandboxCursor` (lease id, generation; M8). `GetProtocolState` lists the terminals. Reattachment by lease and cursor (resume step 5) is proven for terminals in doc 51 E2E-008: the broker outlives the Core (doc 21), the restarted Core attaches under its boot generation, and reads continue from the acknowledged cursor.

## Engineering Memory

Scopes: Run, Session, User, Agent Profile, Repository, Space, Organization. Record types: decision, convention, fact, procedure, failure pattern, dependency knowledge, user preference. Every item stores source provenance, author/actor, confidence, TTL/expiry, scope, sensitivity, supersedes/conflicts links and last validation revision.

Promotion rules:
- transcript summary alone cannot promote;
- web/tool/peer content is untrusted until validated;
- repository facts bind to revision and can stale;
- sensitive memory requires policy-permitted scope;
- user can inspect/delete/supersede where allowed.

## Resume algorithm

1. Acquire session kernel lease with new fencing generation.
2. Load latest projection + event tail and validate hashes.
3. Reconstruct protocol state.
4. Validate latest checkpoint chain against workspace/Git.
5. Reattach terminal/browser/sandbox resources by lease and cursor.
6. Reconcile any `UnknownOutcome` tool calls with Effect Ledger/target state.
7. Reject stale compaction/checkpoint workers.
8. Build fresh Context Projection from current state.
9. Continue from the exact waiting/executing/review boundary.

This flow is release-tested by process kill at every major state.

As built (M4.6): the store carries a kill-point fault plan (`MODBIT_FAULT_KILL_BEFORE_EVENT` / `MODBIT_FAULT_KILL_AFTER_EVENT` = `<EventType>:<n>`; unset in production) that aborts the process right before or right after committing the n-th event of a type, and the kill-point suite runs the reference coding task against every recovery boundary — task start, run, turn, context, model call, tool proposal, dispatch, outcome, file change, plan, checkpoint start and commit, the completion run, the terminal task events — restarting and resuming after each: the same end state and worktree as the unkilled run, one effect, nothing invented, every unknown outcome reconciled. What the suite made atomic: a tool call's outcome lands in one transaction with the retrieval records, the approval it opened and the files it changed; a run's end lands in one transaction with the task transition it implies; a call that already finished is replayed from its recorded result when a resumed run re-enters it. `ReconcileToolCall` lets the user resolve an unknown outcome the Core holds (`EFFECT_CONFIRMED` is never repeated; `EFFECT_ABSENT` may be called again).

## Routing state in the existing durability layers

The conditional plan, its slot table and activation counts, leg/attempt, budget reservation, route epoch and the exact registry, statistics, compiler, gate and realized-risk versions it was compiled with belong to canonical events/protocol projections and checkpoint payloads, not Engineering Memory or transcript reconstruction. Compaction transitions may trigger a recorded re-route only after pending effects reconcile. Preserve active configuration, cache ref, prior profile, route epoch, candidate revision and reservation refs across local/cloud handoff and actual Core kill/restart. Reject old route/compaction/lease generations. Source algorithms: doc 38; proof: EPR-001/008/009 in doc 61.
