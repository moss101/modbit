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
2. Derive PolicyEnvelope from identity, repository, organization rules, permissions, budgets and prior risk.
3. Build intrinsic RequestProfile; join Outcome Statistics and compile/validate cheapest confidence-feasible ConditionalExecutionPlan with all slots reserved before initial execution.
4. Persist plan, routing epoch, active leg and budget reservation using existing event/protocol state.
5. Compile context/prompt/skills/tools for the active solver, reviewer, reviser or escalation leg.
6. Invoke the active model through the Model Gateway; validate requested actions with typed schemas and canonical capability/effect policy.
7. Execute within leg limits, observe the exact result and persist state/events/effects/accounting, including failed attempts.
8. Collect deterministic/static evidence, derive factual required assurance, then independently assess Acceptance Gate evidence before activating any precompiled review/escalation/human slot.
9. Accept, revise, escalate, continue, ask, wait or fail. Admission of ordinary subagents still uses the existing transactional protocol below.

The loop is event-driven and executes one bounded conditional transaction. DIRECT/CASCADE/CRITIQUE are derived path labels only. An active solver/reviser may request governed progressive disclosure. Isolated Non-Committing Reviewers receive revision-bound source/static evidence and bounded disposable execution, with solver hidden reasoning excluded by default. A model may propose completion; Core accepts it only when workflow, policy, verification, evidence and write-safety obligations hold.

Use `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` for workflow semantics and `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` for bounded transitions, persistence and failure algorithms.

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
