# Agent Runtime and Orchestration

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## One runtime, three explicit graphs

Modbit keeps the established terms without building three separate engines:

- **WorkGraph** — tasks/subtasks, dependencies, artifacts and verification gates.
- **AgentGraph** — which logical agents own which work nodes and communication edges.
- **StateGraph** — durable control-state transitions for session/task/turn/tool/subagent.

All are projections coordinated by `core-runtime`; no second orchestration service exists.

As built (M6.1): the AgentGraph and the WorkGraph are projections of the task's log (`crates/domain::agent`, docs/31 `agent_nodes` / `work_nodes`), served as `GetAgentGraph` / `GetWorkGraph` (CLI `task agents`, `task work`). Every task has one primary agent (REQ-EV-0255): `AgentNodeCreated` at its first run with the run and the route's binding, its idempotency key the task id; `AgentNodeTransitioned` through the documented statuses as runs end (`WAITING`, `COMPLETED` when the candidate is with the user, `FAILED`, `CANCELLED`) and resume (`RUNNING`, a finished primary passing through `ADMITTED`, REQ-EV-0050); `AgentBindingChanged` when the run's route or a quality-rejection continuation (EPR-006) moves it to another model while its identity, lineage and run stay (REQ-EV-0256). The Fast Context specialist is a `SPECIALIST` node under the primary, keyed by the call it answers, completed or failed when it returns; admitted subagents join as `SUBAGENT` nodes from M6.3. The WorkGraph is the plan's `steps` (`plan.update`): nodes with a stable id, title, `depends_on`, status (`PENDING | READY | ACTIVE | BLOCKED | DONE | FAILED | CANCELLED`), expected artifacts, verification, evidence references, blockers and attempts, validated whole on every plan version — an unknown dependency, a cycle, a node marked `DONE` without an evidence reference or ahead of a dependency refuses the whole change as `WORK_GRAPH_INVALID` and records no plan version — with readiness following the dependencies and a return to `ACTIVE` after `FAILED` or `BLOCKED` counting an attempt; `WorkNodesChanged` records the nodes as they stand after each version. The graph lives outside the transcript: the model reads its summary in `harness_state.work` every turn, a compaction epoch touches no node and a Core restart rebuilds it from the log exactly (REQ-EV-0052 / 0120).

## Main runtime loop

1. Observe latest task/Run projection under the session kernel lease; evaluate ready WorkGraph nodes.
2. Derive PolicyEnvelope from identity, repository, organization rules, permissions, budgets and protected-surface/mandatory-review/human rules; the envelope carries no learned risk score (ADR-R-052).
3. Build intrinsic RequestProfile; join Outcome Statistics and compile/validate cheapest confidence-feasible ConditionalExecutionPlan with all slots reserved before initial execution.
4. Persist plan, routing epoch, active leg and budget reservation using existing event/protocol state.
5. Compile context/prompt/skills/tools for the active solver, reviewer, reviser or escalation leg.
6. Invoke the active model through the Model Gateway; validate requested actions with typed schemas and canonical capability/effect policy.
7. Execute within leg limits, observe the exact result and persist state/events/effects/accounting, including failed attempts.
8. Collect deterministic/static evidence, derive factual required assurance, then independently assess Acceptance Gate evidence before activating any precompiled review/escalation/human slot.
9. Accept, revise, escalate, continue, ask, wait or fail. Admission of ordinary subagents still uses the existing transactional protocol below.

The loop is event-driven and executes one bounded conditional transaction. DIRECT/CASCADE/CRITIQUE are derived path labels only. An active solver/reviser may request governed progressive disclosure. Isolated Non-Committing Reviewers receive revision-bound source/static evidence and bounded disposable execution, with solver hidden reasoning excluded by default. A model may propose completion; Core accepts it only when workflow, policy, verification, evidence and write-safety obligations hold.

Use `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` for workflow semantics and `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` for bounded transitions, persistence and failure algorithms.

## Competence contracts inside the loop

Steps 5 to 9 execute the competence contracts of `28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`: a plan is recorded before the first write, no file is edited without a retrieval record at the current revision, changes are small revision-bound transactions with tests first where a harness exists, the verification plan is derived and recorded before it runs, every repair attempt is a RepairAttempt record whose repeated equivalent hypothesis escalates through a compiled slot or Needs Attention, and a SelfReview precedes any completion proposal. Core enforces these as policy decisions and events; prompts express them.

## Agent harness contracts (PX-040)

The harness is the part of core-runtime that turns steps 5 to 9 into a loop the model can actually work inside. It is specified here because a competent model in a weak harness loses tasks to truncated output, lost failure evidence, silent budget exhaustion and stale verification. Every rule is enforced by Core (DR-PX-2026-09-05-006).

1. **Turn shape.** Every turn is `ContextCompile → ModelInvoke → validated actions → tool execution → observation`, each persisted as RunSteps before the next begins. A turn never ends because a tool or a test failed: command failure is evidence, not turn failure (`REQ-EV-0099`).
2. **Bounded observations with declared truncation.** Each tool result reaches the model within the inline ceiling of `33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md`; the remainder spills to an OutputRef. The observation states bytes total, exit status, the ranges included and the ranges omitted, so the model knows what it did not see and can page through `artifact.range`. Nothing is dropped from the retained log.
3. **Structured failure channel.** Test and build failures arrive as the normalized `CheckResult`s of `64_VERIFICATION_EXECUTION_CONTRACTS.md` first, then bounded excerpts, then the raw reference (`REQ-EV-0107`). The model never has to parse a runner's log to learn which check failed.
4. **Harness state in context.** Each turn's Context Pack carries a `harness_state` section: the current plan and original write set, the RepairAttempt summary and open failure signatures, quarantined checks, scope counters, remaining turn, tool-call, wall-clock and effect budgets, and the candidate revision. The model reasons over the durable state, not over its memory of the transcript.
5. **Budgets and exhaustion.** Per task: `max_turns` (Alpha default 60), `max_tool_calls` (Alpha default 300), wall clock and effect budgets from the task, and `max_consecutive_no_progress_turns` (Alpha default 3, `RepairPolicy` in doc 28). Exhaustion emits `HarnessBudgetExhausted` and moves the task to `Waiting` or `Needs Attention` with partial evidence; nothing is truncated silently. Defaults are policy-overridable and revisited after the PX-020 baseline.
6. **Environment readiness.** Before the first verification run the harness confirms the configured runners and toolchains resolve in the execution profile; a missing runner is recorded in the plan and the verification plan compensates or a typed question is asked (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`, PX-029). Verification never silently degrades to "no tests found".
7. **Candidate-revision binding.** Every verification result is bound to the candidate revision and environment digest it ran against; a result for a stale revision is recorded and never used for acceptance. The harness refuses a completion proposal while a verification run is in flight or its COMPLETION run predates the last ChangeTransaction.
8. **Revert and checkpoint points.** Before a COMPLETION run and before reverting a `WORSENED` attempt the harness records a checkpoint from M4 onward (as built in M4.3: `before_completion` and `before_revert` checkpoints on the task's log, doc 19); in Alpha, before M4, the revision-bound ChangeTransaction history of the Change Engine is the revert path and the plan says so.
9. **Steering at safe boundaries.** `Steer`, `Pause`, `Cancel` and `Approve` apply between steps; effectful in-flight operations reconcile first. A steer that changes the goal produces a `PlanRevised` with a scope delta, never a silent rewrite of the plan.
10. **Headless resolution.** With no user present (CLI, CI, forge-triggered tasks) typed questions resolve by policy: fail closed to `Needs Attention` by default, auto-allow only where the policy bundle says so, with a configurable wait. A task never blocks forever on a question nobody will answer.
11. **Completion handshake.** Completion proposal → SelfReview → COMPLETION run at the final candidate revision → regression attribution and diff invariants clean → Acceptance Gate. The harness enforces the order; the model can request completion but cannot skip a step.

12. **Typed failure diagnostics** (`REQ-EV-0073`, `REQ-EV-0245`; as built with M4). A failure never reaches the model, the user or an evaluator as a bare status line. One deterministic classifier in core-runtime (`diagnostics`) maps what the source reported — a tool result's status and code, a provider error code, a store error, a budget, a loop boundary, a restart boundary, a lost lease — to a `FailureDiagnostic`: `class` (`TIMEOUT`, `INFRASTRUCTURE`, `APPLICATION`, `POLICY`, `APPROVAL`, `CORRUPT_STATE`, `UNKNOWN_OUTCOME`, `LEASE`, `PROVIDER`, `BUDGET`, `HARNESS`, `CANCELLED`, `INVALID`), a stable `code` within it, `retryable`, `user_action` (empty when nothing only the user can do), `recovery_path`, `evidence_refs` and sorted `features` tags an evaluator keys on (`class:`, `code:`, `source:`, `retryable`/`not_retryable`, and the source's own). Every non-success observation ends with the rendered diagnosis (`failure_class`, `retryable`, `recovery`, `user_action`); every `TaskNeedsAttention` carries it typed; `TaskStatus` reports the latest one so the CLI and the desktop show class, retryability, action and recovery instead of a guess. A timeout is `TIMEOUT` and retryable, never a success; a stored object that no longer matches its digest is `CORRUPT_STATE` and not retryable, told apart from a missing artifact; a dispatched effect the Core cannot see the end of is `UNKNOWN_OUTCOME`, reconciled and never retried. The fault corpus (`crates/core-runtime/tests/fixtures/diagnostics`) pins the diagnosis of every source kind byte for byte.

Harness contracts are qualified by PX-E2E-040 and measured through the metrics of doc 63; they add no subsystem and no new tool family.

## Decomposition

The primary agent may propose `SubtaskSpec` values containing: objective, expected artifacts, dependencies, read scope, proposed write scope, required tools, execution profile, verification condition and budget.

Core validates rather than trusting model-generated parallelism.

## Transactional subagent admission

Admission transaction must succeed as one operation:

1. capacity ticket available;
2. parent task still active at expected generation;
3. proposed write set has no unsafe conflict with admitted workers;
4. worktree or immutable snapshot allocated;
5. capability lease minted with least privilege;
6. sandbox/terminal/browser resources within quota;
7. AgentGraph node + WorkGraph ownership persisted.

If any step fails, no worker starts and no partial reservation leaks.

As built (M6.3, M6.5): the primary proposes a `SubtaskSpec` through the harness tool `agent.spawn` (objective, expected artifacts, dependencies, read and write scope, required tools, verification, turn and tool budgets, the work node it will own, `FOREGROUND` | `BACKGROUND`, an idempotency key). `services/modbit-core/src/spawn.rs` admits it as one transaction, in this order and refusing at the first step that fails with everything taken before it returned: (1) idempotency — a key that names an admitted child reattaches to it (REQ-EV-0007); (2) depth — the parent's depth plus one must not exceed `MODBIT_AGENT_MAX_DEPTH` (1 by default: nested delegation is disabled, `NESTING_DISABLED` / `DEPTH_EXCEEDED`, REQ-EV-0051; a child sees no `agent.*` tool at all, REQ-EV-0219); (3) the parent is `Running` and the session's lease generation is the one it started under (`PARENT_NOT_ACTIVE`); (4) the write scope overlaps no live child's (`WRITE_CONFLICT` naming the pairs; an unbounded scope cannot join admitted workers) — the explicit-path check, M6.4 adds the semantic rules; (5) a capacity ticket for the child's run (`CAPACITY_EXHAUSTED`); (6) a checkpoint of the parent and a fork of it into the child's own task and worktree carrying nothing of the parent's plan, decisions, evidence or context (`TaskOrigin::Subagent`), leased for its profile with `fs.write` narrowed to the write scope, no `git.worktree`, and the effect ceiling capped at `REVERSIBLE_WRITE` (REQ-EV-0046 / 0048 / 0078); (7) the `AgentExecutionCapsule` object, the `SUBAGENT` node under the parent's primary with the child task, and the work node it owns, recorded together (`SubagentAdmitted`; `SubagentCapsuleBound` on the child's log); (8) the child's run started on the admission ticket. A failure after the worktree removes it, cancels the child task and returns the ticket (`SubagentAdmissionRefused` with `stage` and `rolled_back`, exercised by `MODBIT_FAULT_SPAWN=WORKTREE`, REQ-EV-0267). The child's harness runs inside the capsule after any restart: writes outside the write scope are refused `WRITE_SCOPE_DENIED` before any effector even when its own plan names them, its projection is narrowed to the capsule's tools when named, and its harness state carries the capsule and its note. When the child's loop ends — whatever way — `SubagentResultRecorded` lands on the parent's log with the `SubagentResult` object (status, the completion summary, the paths changed in the worktree, verification-run and gate references, unresolved risks, the branch and worktree, the candidate revision) and the child's node moves to `COMPLETED` / `FAILED` / `CANCELLED` / `WAITING`; the parent collects it with `agent.wait` (bounded, by agent id or the spawn key) or `agent.result`, and cancels with `agent.cancel`. The envelope is untrusted context: the parent reads the branch and merges through the Change Engine. `FOREGROUND` waits inside the spawning turn, bounded by the same ceiling.

As built (M6.7, docs/25 "Subagent continuation"): a background child outlives the Core that started it. Startup reconciliation suspends the child's run at its boundary exactly as the parent's (`RunSuspended`, `TaskWaiting`, `TaskNeedsAttention`) and moves the agent nodes with it — the task's `PRIMARY` and, for a subagent's task, its `SUBAGENT` node on the parent's graph — to `WAITING` (`AgentNodeTransitioned`, "Core restarted; the run is suspended at its boundary"), so the parent and its Fleet card see the child as suspended with its identity, lineage and capsule intact. Resuming the parent (`StartTask`) resumes every child a restart left suspended (`spawn::resume_suspended_children`): the same child task, agent id, capsule and run continue on their own log (`TaskResumed`, `RunResumed`; the capsule's budgets; a fresh capacity ticket on the child's log, the admission ticket having lapsed with the dead Core), the node returns to `BACKGROUND` / `RUNNING`, and the parent's re-entered `agent.wait` collects the result envelope when the child ends. A parent that asks for the same idempotency key again while the child is suspended reattaches and resumes it the same way (`spawn` step 1). Nothing runs twice: the child's dangling calls are re-entered by id and a read-only call the dead Core had dispatched is cancelled and proposed afresh (docs/19 resume step 6). An unset child tool budget is the runtime's default (300), never "none". Test: `m6_7_a_background_child_survives_a_core_restart_and_hands_its_result_back`.

## Semantic conflict detection

Before parallel builders start, Modbit checks:
- explicit file/path overlap;
- same symbol or public interface ownership from AST/LSP graph;
- dependency hot spots;
- migration/schema/shared config files;
- generated files and lockfiles;
- test fixtures likely to conflict.

Read-only investigative agents can share an immutable repository revision. Builders default to separate Git worktrees.

As built (M6.4): `crates/core-runtime::conflict` decides, before a builder starts, what its write scope conflicts with among the workers already admitted, from what the parent's repository index knows (`RepoFacts`: the indexed paths, the public symbols each file defines from the tree-sitter symbol index, the direct importers of each file from the evidence graph). Rules: `PATH_OVERLAP` (a scope entry — path, prefix or glob — covers the other's; blocks); `SYMBOL_OWNERSHIP` (a public symbol or interface — function, method, class, struct, enum, interface, trait, type, const, module, keyed by container — defined in files of both scopes; blocks); `HOT_SPOT` (a file imported by at least `hot_spot_importers` others, 5 by default: blocks when the candidate would hold it and the other worker's files import it, warns when the other holds it and the candidate depends on it); `SHARED_FILE` (migrations, schemas, `Cargo.toml`/`Cargo.lock`, `package.json`, lockfiles, `tsconfig.json`, CI workflows — one worker at a time while any other is live; blocks); `GENERATED_FILE` (`**/generated/**`, `**/gen/**`, `*_pb.*`, `*.gen.*`; blocks); `TEST_FIXTURE` (fixtures under the same directory in both scopes; blocks). Admission (`spawn.rs` step 4) refuses `WRITE_CONFLICT` naming every blocking rule, its subject and the worker, and hands the warnings to the parent in the `agent.spawn` result (`conflict_warnings`). Read-only investigators need no write scope; builders default to separate worktrees, as the fork gives them.

## Capacity tickets

Capacity is a typed resource vector, not a simple agent count: model concurrency, terminal slots, sandbox slots, browser slots, memory budget and provider quota. Tickets have lease expiry and generation fencing.

As built (M6.2): the pool is `crates/core-runtime::capacity` — a `ResourceVector` of model concurrency, terminal slots, sandbox slots, browser slots, memory (MiB) and provider quota, configured by `MODBIT_CAPACITY` (`model=4,terminal=8,sandbox=2,browser=2,memory_mib=8192,provider=8` by default; a malformed spec refuses startup) with a ticket lease of `MODBIT_CAPACITY_TTL_MS` (120 s by default). `CapacityPool::allocate` is all or nothing: the first dimension that does not fit refuses the whole request (`CAPACITY_EXHAUSTED` naming the dimension, the units needed and available, and the live holders; `EXCEEDS_POOL` for a request the host could never satisfy) and reserves nothing. A run consumes one model slot and one unit of provider quota (`ResourceVector::one_run`) in `Runtime::start` before its run record exists, so a refused `StartTask` is a typed refusal plus a `CapacityDenied` record on the task and nothing else — no run, no agent node, no lease, no worktree; the task keeps its state and starts once capacity returns. The ticket (`CapacityTicketGranted`) is renewed at every turn boundary under the run's lease generation — a stale owner cannot renew — and lapses at its expiry (`CapacityTicketReleased` reason `LAPSED`) if the run is away longer, which frees the slot for a waiting task; a run whose ticket lapsed takes one again at its next boundary when the pool allows and otherwise suspends `Waiting(Capacity)` with the typed diagnosis `CAPACITY_LOST` (retryable; `StartTask` takes a ticket again). The loop's end releases the ticket (`RELEASED`). `GetCapacity` (CLI `capacity show`) serves limits, held, available, the lease length and the live tickets. Terminal, sandbox and browser slots are dimensions of the same vector; the owners that open them take their tickets as they arrive on the subagent path (M6.3).

## Steering

User actions are durable control events: `Steer`, `Pause`, `Resume`, `Cancel`, `Approve`, `Deny`, `TakeBrowserControl`, `ReturnBrowserControl`. Steering is applied at safe control boundaries; effectful in-flight operations are reconciled before cancellation completes.

## Agent-to-agent communication

Subagents return structured `SubagentResult` with summary, evidence refs, artifacts, unresolved risks and proposed follow-ups. Peer messages are untrusted context until the parent Context Engine selects them. A peer message never silently becomes durable memory.

## Verification roles

A read-only reviewer agent may be used when useful, but verification gates are deterministic where possible: tests, diagnostics, builds, type checks, security rules, diff invariants and evidence completeness. Reviewer model output is advisory unless backed by evidence.

## Budgeting

Each node has token, tool-call, wall-clock and effect budgets. Runtime emits measured savings/costs. Budget exhaustion moves task to `Waiting` or `Needs Attention`, not silent truncation.


## V2 agent-runtime reconciliation requirements

The runtime additionally guarantees: persisted AgentNode identity; idempotent spawn; foreground/background transitions without restart; park/resume; bounded recursive delegation; typed `AgentResultEnvelope`; WorkGraph/TodoState outside transcript; stall detection; SessionLease fencing; and typed input dispatch (`STEER/COLLECT/FOLLOW_UP`). A source product's background default is not copied as policy: the Modbit scheduler backgrounds only separable work whose result is not a dependency of the next parent step.

## Compound execution and interruption

Bind all legs to the existing Run/Session, capacity, cancellation and effect ledgers. Escalation/revision/fallback consume remaining request budget after spent/in-flight costs and mandatory verification reserve. No per-leg reset. Autonomous mutations use existing isolated worktree/checkpoint and compare-and-apply; failed or cancelled workflows leave no partially accepted patch. External irreversible effects require ordinary protected-effect reconciliation and are not undone by discarding a worktree.

Persist plan/leg/attempt and routing epoch before dispatch. At restart, reconcile pending tools, usage and effects before resuming a valid leg boundary; stale epoch/lease/revision results are recorded but rejected. An unavailable mandatory gate or reviewer prevents acceptance. Optional review may be omitted only if the already compiled slot/acceptance rules allow it. Missing required slots cannot be synthesized; end/reconcile the transaction before separate admission with remaining request budget. See EPR-005..009 and EPR-FI-005..009 in docs 49/61.
