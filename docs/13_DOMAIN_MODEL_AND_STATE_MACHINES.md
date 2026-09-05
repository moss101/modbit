# Canonical Domain Model and State Machines

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Identity types

All IDs are opaque 128-bit UUID/ULID-style values encoded canonically. Never overload user-visible names as identifiers.

`TenantId, UserId, SpaceId, WorkspaceId, RepositoryId, SessionId, TaskId, RunId, TurnId, RunStepId, AgentId, SubagentId, ToolCallId, ApprovalId, CapabilityLeaseId, EffectId, CheckpointId, CompactionEpochId, OutputRefId, BrowserSessionId, TerminalSessionId, SandboxLeaseId`.

## Core aggregates

### Session
Long-lived durable interaction container. A session may contain multiple tasks/turns and can survive app restarts.

### Task
User goal with workspace snapshot, policy profile, execution preference and completion criteria.

### Run
One concrete execution attempt/continuation of a task. A task can have multiple runs after retry/fork/handoff.

### Turn
One model interaction cycle, including context compile, model stream and resulting tool/procedural activity.

### RunStep
Typed atomic runtime step: `ContextCompile`, `ModelInvoke`, `ToolCall`, `ProcedureRun`, `ApprovalWait`, `Verification`, `Checkpoint`, `Handoff`, `UserQuestion`, and the competence steps `Plan`, `RepairAttempt` and `SelfReview` (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`).

### Agent node
Logical reasoning actor bound to task/subtask, model policy, capabilities and workspace scope. It is not a process identity.

## State machines

### Task
```text
Created → Queued → Running ↔ Waiting
                    │  ├─ UserInput
                    │  ├─ Approval
                    │  ├─ Capacity
                    │  ├─ External
                    │  └─ Provider
                    ├→ ReadyForReview → Completed
                    ├→ Failed
                    └→ Cancelled
```

### Turn
`Prepared → Streaming → Executing → Verifying → Completed | Interrupted | Failed`.
A tool failure can transition `Executing → Streaming/Executing` for repair without failing the turn.

### Tool call
`Proposed → Validated → PolicyChecked → ApprovalPending? → Dispatched → Streaming → Succeeded | Failed | Cancelled | UnknownOutcome`.
`UnknownOutcome` is never automatically retried for effectful tools; query Effect Ledger/target state first.

### Approval
`Requested → Approved | Denied | Expired | Superseded`. Approval binds the normalized effect intent hash, not merely a tool name.

### Subagent
`Proposed → AdmissionPending → Admitted → Running → Waiting | Completed | Failed | Cancelled`. Admission atomically acquires capacity ticket, capability scope and workspace isolation.

### Browser control lease
`AgentControlled ↔ UserControlled → Released`. Agent input is blocked while user owns the lease.

## Canonical event envelope

Every state transition is emitted as an immutable event:

```text
EventEnvelope {
  event_id
  tenant_id
  session_id
  task_id?
  run_id?
  turn_id?
  step_id?
  aggregate_type
  aggregate_id
  sequence
  event_type
  schema_version
  occurred_at
  actor
  causation_id?
  correlation_id?
  payload_ref_or_inline
  integrity_hash
}
```

Ordering is guaranteed per aggregate sequence, not globally. Projection consumers must be idempotent.

## Fencing and epochs

Every mutable recoverable stream includes a monotonic generation where stale writers are dangerous:
- session kernel lease generation;
- checkpoint epoch;
- compaction epoch;
- terminal replay generation;
- browser control lease generation;
- sandbox lease generation.

A result with an older generation is rejected, recorded and never applied silently.

## Invariants

- Only Event Store append + transactional projection update can advance authoritative state.
- UI-provided state is advisory and may never overwrite Core state.
- Tool call IDs are stable across retry/reconnect.
- Effectful retries require idempotency proof or explicit reconciliation.
- Completion requires verification result references, not an LLM string saying “done”.

## Versioned execution plans on the canonical Run

Add PlanId, LegId, InvocationAttemptId and RoutingEpoch identities without changing Session/Task/Run/Agent lineage. Run holds RequestProfile/PolicyEnvelope/ConditionalExecutionPlan refs, active leg, escalation/revision state, gate/reviewer/realized-risk refs, spent/reserved budget and accounting completeness. RoutingSessionState is a projection of existing session state, not a new durability layer. Routing epoch, kernel lease and candidate revision fence every returned result.

Plan lifecycle: `Compiled → Validated → Executing → Verifying → Accepted | Failed | Cancelled | Superseded`; compilation/validation failure has no dispatch. Leg lifecycle: `Pending → Running → Succeeded | Failed | Cancelled | UnknownOutcome`. A superseded plan keeps its attempts/cost and cannot apply late output. The admitted conditional slot graph determines allowable next legs; Acceptance Gate INCONCLUSIVE/REJECT is not successful completion. These are domain states, separate from build-agent lifecycle states.

RunStep adds `RequestProfile`, `PlanCompile`, `ContinuationActivate`, `AcceptanceGate`, `Review`, `RealizedRisk` and `AccountingReconcile` as typed existing-runtime steps. New Runs require plan provenance; legacy Runs remain explicitly legacy-direct until migrated at a safe boundary. Required wire fields, bounded transitions and failure semantics are in `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`.

## Conditional v1.1 domain refinements

ConditionalExecutionPlan schema 2 adds transaction and continuation-slot identity, predecessor/trigger, maximum activation counts, full reservation and validation digest before first dispatch. State projections retain separate RealizedRisk (required assurance), AcceptanceGateResult (evidence sufficiency), ReviewerResult and RoutingDecisionRecord with statistics/gate/risk versions. Runtime activates only precompiled slots; a missing required slot emits REQUIRED_CONTINUATION_UNAVAILABLE. Different topology needs reconciled separate admission with remaining budget/new epoch. DIRECT/CASCADE/CRITIQUE are executed-path classifications and never dispatch enums. Legacy adapters and exact transitions are in doc 38.
