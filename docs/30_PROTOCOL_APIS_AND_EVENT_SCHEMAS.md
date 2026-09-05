# Protocol, APIs, and Event Schemas

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Schema strategy

Use versioned Protobuf definitions as the canonical cross-process schema for local Core, cloud workers, Sandbox Gateway and guest RPC. TypeScript and Rust bindings are generated in CI. Human-facing cloud control endpoints use JSON over HTTPS but map one-to-one to canonical domain commands/events.

Breaking schema changes require a new major protocol version; additive fields use backward-compatible numbering. Every persisted event stores `schema_version`.

## Local SurfaceProtocol

Transport: authenticated framed Protobuf over Unix domain socket on macOS/Linux and named pipe on Windows. Electron main is the only desktop process permitted to connect. At Core startup the main process and Core exchange a boot-scoped random secret through inherited pipe/secure process channel; the secret is never exposed to renderer.

### Commands

```text
CreateSession
CreateTask
GetSessionSnapshot
SubscribeEvents(cursor)
SteerTask
PauseTask
ResumeTask
CancelTask
RespondToQuestion
ApproveEffect
DenyEffect
CreateBrowserSession
TakeBrowserControl
ReturnBrowserControl
OpenCodeReference
ListArtifacts
ReadOutputRef(range)
StartCloudHandoff
GetSettings
UpdateSettings
```

Every command carries `command_id`, authenticated principal, expected aggregate generation when mutating state, and client timestamp. Mutating commands are idempotent by `command_id`.

## Cloud HTTP control API

Base prefix: `/v1`.

| Method | Path | Purpose |
|---|---|---|
| POST | `/v1/sessions` | create cloud-visible session |
| GET | `/v1/sessions/{session_id}` | current projection |
| POST | `/v1/sessions/{session_id}/tasks` | create task |
| POST | `/v1/tasks/{task_id}:steer` | durable steer event |
| POST | `/v1/tasks/{task_id}:pause` | pause |
| POST | `/v1/tasks/{task_id}:resume` | resume |
| POST | `/v1/tasks/{task_id}:cancel` | cancel |
| POST | `/v1/approvals/{approval_id}:approve` | approve bound intent |
| POST | `/v1/approvals/{approval_id}:deny` | deny |
| GET | `/v1/events?session_id=...&after=...` | cursor replay |
| GET | `/v1/outputs/{output_ref_id}` | ranged output read |
| POST | `/v1/handoffs` | local→cloud checkpoint handoff |
| GET | `/v1/artifacts/{artifact_id}` | artifact metadata/access grant |

Event streaming is WSS `/v1/stream` with authenticated subscription and resume cursor. If WebSocket is blocked, client falls back to paginated event replay; task execution does not depend on a permanently open socket.

## Canonical command envelope

```text
CommandEnvelope {
  command_id
  tenant_id
  user_id
  session_id?
  aggregate_id?
  expected_generation?
  command_type
  schema_version
  payload
  issued_at
}
```

## Canonical event types

### Session/task
`SessionCreated, TaskCreated, TaskQueued, TaskStarted, TaskWaiting, TaskNeedsAttention, TaskReadyForReview, TaskCompleted, TaskFailed, TaskCancelled, TaskSteered`.

### Turn/model
`TurnPrepared, ContextPackCompiled, ModelInvocationStarted, ModelDeltaReceived, ToolProjectionSelected, ModelUsageRecorded, ModelInvocationCompleted, TurnInterrupted, TurnCompleted, TurnFailed`.

### Tool/procedure
`ToolCallProposed, ToolCallValidated, ToolCallPolicyDecision, ToolCallDispatched, ToolOutputDelta, ToolCallSucceeded, ToolCallFailed, ToolCallUnknownOutcome, ProcedureStarted, ProcedureCompleted, ProcedureFailed`.

### Workspace/execution
`WorkspaceRevisionAdvanced, FileChanged, GitStateChanged, TerminalCreated, TerminalOutputAdvanced, ProcessExited, SandboxLeaseAcquired, SandboxLost, BrowserSessionCreated, BrowserStateAdvanced, BrowserControlTransferred`.

### Durability
`CheckpointStarted, CheckpointCommitted, CheckpointRejectedStale, CompactionStarted, CompactionCommitted, CompactionRejectedStale, MemoryItemPromoted, MemoryItemSuperseded`.

### Security/effects
`CapabilityLeaseGranted, CapabilityLeaseRevoked, ApprovalRequested, ApprovalResolved, EffectReceiptAppended, SecretHandleUsed, EmergencyStopActivated`.

## Tool call wire schema

```text
ToolCallRequest {
  tool_call_id
  tool_name
  tool_version
  arguments_json
  capability_lease_id
  execution_profile
  expected_workspace_revision?
  timeout_ms
  output_budget_bytes
}
```

Tool result always distinguishes application outcome from transport/runtime outcome:

```text
ToolCallResult {
  status: SUCCESS | APPLICATION_FAILURE | INFRA_FAILURE | CANCELLED | UNKNOWN_OUTCOME
  structured_output_json?
  stdout_ref?
  stderr_ref?
  produced_artifact_ids[]
  effect_receipt_ids[]
  workspace_revision_after?
}
```

## OutputRef API

An OutputRef is immutable. Reads accept `offset` + `length` and return checksum, total bytes, content type and selected range. Renderer/model never automatically loads the full object.

## Version compatibility

Desktop startup performs protocol capability negotiation with Core. If major versions differ, task mutation is blocked with an explicit upgrade requirement; read-only export remains available when possible. Guest/Gateway negotiate their method set before a sandbox is admitted.

## Execution policy protocol extension

Add `SetExecutionPreference` (objective profile or allowed manual pin) to existing authenticated command transport; it carries command ID, expected Run generation and user intent only. Add `GetRoutingDiagnostics` as an authorized redacted projection read. Privileged control-plane policy/registry publication and activation require authenticated admin entitlement, signed immutable bundle and compare-and-swap expected version; they are not renderer-owned settings writes.

Add canonical events `RequestProfileRecorded`, `ExecutionPlanCompiled`, `ExecutionPlanRejected`, `ExecutionPlanActivated`, `ExecutionPlanSuperseded`, `PlanLegStarted`, `PlanLegCompleted`, `AcceptanceGateEvaluated`, `ReviewerResultRecorded`, `RealizedRiskDerived`, `EscalationRequested`, `RoutingEpochAdvanced`, `RoutingAccountingReconciled`, `RoutingOutcomeRecorded`, `RoutingPolicyActivated`, `RoutingPolicyRolledBack`. Every event uses the existing envelope with tenant/Run/causation/sequence/schema identity; payloads bind plan/leg/attempt/epoch and revision where applicable. Policy events bind actor, evidence bundle and previous/new versions. Large refs use existing content-addressed artifacts.

Generate the eight routing contracts from versioned schemas per `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`. Add optional fields compatibly to old envelopes; never reuse field numbers. Unsupported plan major/workflow semantics fail negotiation before mutation. Redacted client events show objectives, verification/escalation progress, effective cost and permitted model labels without raw profiles/prompts/weights/credentials. Replay from cursor must reproduce state without re-executing inference or effects.

## v1.1 conditional wire contract and compatibility

New writers emit ConditionalExecutionPlan schema 2 plus RealizedRisk, AcceptanceGateResult, ReviewerResult, RoutingDecisionRecord and independently versioned OutcomeStatistics references. Add `ContinuationSlotActivated`, `ContinuationSlotRejected`, `AcceptanceGateEvaluated`, `OutcomeStatisticsPublished` and `QualityFloorInfeasible` under the existing tenant/Run/sequence/causation envelope. Bind transaction/slot/activation ordinal/leg/attempt/epoch, candidate and compiler/statistics/gate/risk versions. Existing quality/critic event names decode as explicit legacy aliases; no event name authorizes a capability or runtime topology.

Record candidate quality mean/LCB/cost/latency/eligibility/exclusions and actual path. Client projections disclose QUALITY_FLOOR_INFEASIBLE without claiming target satisfaction, even when actual evidence later permits acceptance. A v1.0 template cannot execute without lossless validated migration. Incompatible version negotiation blocks mutation. Commands can carry user objectives/approval but cannot introduce new continuation slots in an executing transaction.
