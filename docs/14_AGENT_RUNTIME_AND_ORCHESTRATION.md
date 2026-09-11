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

## Semantic conflict detection

Before parallel builders start, Modbit checks:
- explicit file/path overlap;
- same symbol or public interface ownership from AST/LSP graph;
- dependency hot spots;
- migration/schema/shared config files;
- generated files and lockfiles;
- test fixtures likely to conflict.

Read-only investigative agents can share an immutable repository revision. Builders default to separate Git worktrees.

## Capacity tickets

Capacity is a typed resource vector, not a simple agent count: model concurrency, terminal slots, sandbox slots, browser slots, memory budget and provider quota. Tickets have lease expiry and generation fencing.

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
