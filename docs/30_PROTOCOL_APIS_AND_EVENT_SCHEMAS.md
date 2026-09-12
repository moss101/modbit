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

### Competence
`PlanRecorded, PlanRevised, RetrievalRecorded, RepairAttemptRecorded, RepairAttemptConcluded, RepairEscalated, ReproductionRecorded, ContextEpochOpened, SelfReviewRecorded, ScopeExpansionRecorded, NoProgressDetected, HarnessBudgetExhausted, ToolsActivated` (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`, `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`) and the verification execution events `VerificationBaselineRecorded, VerificationRunRecorded, FlakyCheckQuarantined, RegressionAttributed, DiffInvariantViolated` (`64_VERIFICATION_EXECUTION_CONTRACTS.md`). Payloads bind run, candidate revision, environment digest and, where applicable, check ids, invariant ids and scope counters; raw reports are content-addressed artifacts.

### Durability
`CheckpointStarted, CheckpointCommitted, CheckpointRejectedStale, CompactionStarted, CompactionCommitted, CompactionRejectedStale, MemoryItemPromoted, MemoryItemSuperseded`.

As built (M4): plus `RouteReevaluated` (run; EPR-009: boundary, routing epoch, the binding in force before and after, STAY | SWITCH | INITIAL, stay and switch totals, the itemized switch cost, the cache state consulted), `RealizedRiskDerived` (run; EPR-008: level, minimum assurance, obligations, reasons, policy and rules versions, the record's object hash), `AcceptanceGateEvaluated` (run; EPR-017: verdict, required assurance, missing evidence, reject reasons, gate and risk refs and versions, trigger `COMPLETION_RUN` | `REVIEW_DECISION`; valid on a completed run), `CheckpointRestored` (with `preconditions_checked`), `ContextEpochOpened`, `ProtocolStateResumed`, `ToolCallReconciled`, `TerminalCreated`, `TerminalOutputAdvanced`, `ProcessExited`, `SessionBranched` (session; `kind` fork | revert | cancel) and `TaskForked` (the fork's lineage: source task, checkpoint, epoch, cursor, capsule, carried counts, worktree, branch; doc 19 "Session branching"). Commands added for the durability surface: `GetProtocolState`, `ReconcileToolCall`, `CreateCheckpoint`, `ListCheckpoints`, `RestoreCheckpoint` (with optimistic `expected` hashes), `ForkTask`, `PreviewRewind`, `GetSessionTree`, `GetTaskAssurance`, `GetRoutingSessionState`.

As built (M5.1): `ToolProjectionSelected` (turn) carries, besides the projection hash, `projected` (the tool names offered that turn), `withheld` (`<tool>:<reason>` — `DECLARE_WRITES` | `DECLARE_PROTECTED_EFFECT` | `REVIEWER_LEG`) and `leg_role`; a model call outside the turn's projection fails at the policy stage with `TOOL_NOT_PROJECTED` (doc 16 "Projection as built").

As built (M5.4): task events `ProgramStarted` (handle, program_ref, declared_effects, bindings, budget) and `ProgramEnded` (handle, status COMPLETED | FAILED | BUDGET_EXHAUSTED | CANCELLED, budget_exhausted, outcome_ref, tool_calls, elapsed_ms, interrupt_polls); a program's binding calls are ordinary tool-call aggregates whose `call_id` is `<exec call id>#<n>`; exec/wait are `ProcedureRun` steps (doc 16 "Bindings and the model-visible surface as built").

As built (M5.5): task events `SkillSelected` (name, version, content_hash, lifecycle, source, reason, instructions_hash, instructions_truncated, tool_projection, tools_unavailable) and `SkillRejected` (name, code, reason); `StartTask.skills` names skills explicitly (doc 16 "Skills as built"). Task event `RulesSelected` (active rules with id, layer, source, hash and reason; dormant; expired; conflicts with winner and loser sources; invalid files) whenever a turn's rule selection changes (doc 16 "Rules as built").

As built (M5 media): task event `MediaBridged` (digest, mime, routed_model, bridge_endpoint, bridge_model, description_ref, input_tokens, output_tokens, error) whenever the vision bridge described media for a text-only routed model, once per distinct digest in a run; the description is an object the record names, never transcript text the log holds twice (doc 25 "As built").

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

## Thin-client and source-control extension (DR-PX-2026-09-05)

Add commands `SubmitExternalDiagnostics` (source identity and version, workspace and file revision, ranges, messages; provenance external_ide), `ApplyUserPatch` (revision-preconditioned ChangeTransaction with provenance user_direct_edit), `OpenPullRequest` and `UpdatePullRequest` (protected external effects through `forge.pr.*`, bound to a candidate revision) and an `origin` field on `CreateTask` (`desktop`, `cli`, `ide_adapter`, `forge_issue`, `forge_webhook`). All are ordinary authenticated commands with `command_id`, principal and expected generation; none grants capability.

Add events `ExternalDiagnosticsRecorded`, `ExternalDiagnosticsRejected`, `UserPatchApplied`, `ForgePullRequestOpened`, `ForgePullRequestUpdated`, `ForgeReviewCommentSteered`, `CiResultRecorded` and `TaskCreatedFromIssue`, each under the existing envelope with revision, receipt and provenance fields. Thin clients consume these by cursor like any other event. The Cloud API adds `POST /v1/forge/webhooks/github` with signature verification, replay protection and tenant mapping that issues the same canonical commands. Contract owners and qualifications: `29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`, PX-001..011 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md`.
