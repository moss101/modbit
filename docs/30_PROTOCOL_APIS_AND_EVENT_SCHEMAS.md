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

As built (M8.1, `apps/cloud-api`): `POST /v1/auth/token {secret}` and `POST /v1/auth/refresh {refresh_token}` issue the token pair; every other route needs `Authorization: Bearer <access token>` and is rate-limited per principal (`429 RATE_LIMITED`). Mutating requests carry `command_id` (uuid) in the body and replay their recorded outcome with `x-modbit-replayed: true`. `POST /v1/sessions`, `GET /v1/sessions/{id}` (the projection, its tasks, its lease and `last_session_offset`), `POST /v1/sessions/{id}/tasks {goal_text, execution_profile?, workspace_root?}` (`TaskCreated` + `TaskQueued`, the session marked ready for a worker), `POST /v1/tasks/{id}:steer {text}` (`TaskInputQueued`, mode STEER), `:pause` / `:resume` (`TaskPauseRequested` / `TaskResumeRequested`, for the execution owner), `:cancel` (`TaskCancelled` at once when no worker holds the session and the task has not run; `TaskCancelRequested` otherwise), `POST /v1/approvals/{id}:approve|:deny {intent_hash, reason}` (`409 INTENT_MISMATCH` for any other hash, `409 APPROVAL_NOT_OPEN` for a second decision), `GET /v1/events?session_id&after&limit` (cursor replay by `session_offset`, `next_after`), `GET /v1/outputs/{hash}?start&len` or `Range: bytes=` (`206` with `Content-Range`), `GET /v1/artifacts/{hash}` (metadata and a five-minute signed URL), WSS `/v1/stream?session_id&after` (frames `{"event": …}` after the cursor, `{"caught_up": n}`, then live). Errors are `{code, message}` with `x-request-id` on every response; `/v1/handoffs` follows with M8.7.

As built (M8.2): while a worker holds the session's lease, every mutating route for that session or its tasks and approvals is relayed to the worker rather than applied: `202 {status: PENDING, command_id, relayed_to: {worker_id, generation}}`, the command recorded in the ledger with its body; `GET /v1/commands/{command_id}` reads its state and, once the worker completed it, the worker's outcome (`ACCEPTED` with `result`, or `REJECTED` with `code` — `PAUSE_UNSUPPORTED` for `:pause`/`:resume`, which the Core has no state for); a retry of a pending command replays `202`. A task created through the API is named by its creating `command_id` (`task_id = command_id`), applied by the API or by the worker alike. The worker's Core speaks two commands no other client kind holds (`session.mirror`): `ImportMirroredEvents {events: [{envelope_json, payload_json}]}` → `MirroredEventsImported {imported, already_present, last_offset}` and `ReadMirrorEvents {session_id, after_offset, limit}` → `MirrorEvents {events, last_offset}`; `TaskView` carries `workspace_root`.

As built (M8.7, docs/21 "Handoff local → cloud"): `PUT /v1/objects` (raw body, `content-type` kept) stores an object in the caller's tenant → `201 {hash, bytes}`; `POST /v1/handoffs {command_id, manifest, parts: {"events.jsonl": hash, "manifest.json": hash, "repo.bundle"?: hash}}` admits a local task's continuation → `201 {session_id, task_id, bundle_hash, local_manifest_hash, imported_through, capabilities}` (a retry replays `200`, `x-modbit-replayed`); `409 CAPABILITY_PARITY {missing}` when the manifest's `capabilities` name what `cloud_isolated` does not serve (the cloud serves `fs.read`, `fs.write`, `git.read`, `shell.exec`, `network.egress`, `secret.use`), `422 PART_MISSING` when a part was not uploaded, `409 HANDOFF_LOG_INTEGRITY` / `HANDOFF_LOG_FORK` when the log does not chain or the session is another tenant's. The local Core speaks `ExportHandoff {task_id, out_dir}` → `HandoffExported {task_id, bundle_dir, manifest_json, manifest_hash, parts}` (`task.author`); the worker's Core speaks `ImportObjects {objects}` → `ObjectsImported {hashes}` and `RebindTaskWorkspace {task_id, workspace_root, reason, execution_profile}` → `TaskWorkspaceRebound {task_id, offset}` (`session.mirror`), and `ImportMirroredEvents.admitted_handoff` lets it import the origin tenant's envelopes. Task events: `TaskHandedOff {bundle_hash, checkpoint_id, git_head, capabilities, secret_handles}` (the laptop's, after the bundle), `TaskHandoffAdmitted {bundle_hash, from_tenant, capabilities}` (the cloud's), `TaskWorkspaceRebound {workspace_root, reason, execution_profile}` (the worker's; it moves the task's root and profile).

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

As built (M6.1): task events `AgentNodeCreated` (node: agent_id, task_id, parent_agent_id, root_agent_id, depth, kind PRIMARY | SUBAGENT | SPECIALIST, status, run_id, capsule_ref, binding {endpoint, model}, idempotency_key, owns), `AgentNodeTransitioned` (agent_id, from, to, run_id, reason), `AgentBindingChanged` (agent_id, from, to, reason) and `WorkNodesChanged` (plan_version, changed: the work nodes as they now stand, ready: ids whose readiness followed); commands `GetWorkGraph` → `WorkGraphView` and `GetAgentGraph` → `AgentGraphView` (doc 14 "As built (M6.1)"). `plan.update` gains `steps` (work node changes). All record-only.

As built (M6.3 / M6.5): task events `SubagentAdmitted` (agent_id, child_task_id, capsule_ref, ticket_id, worktree, branch, write_scope, work_node, idempotency_key, mode), `SubagentAdmissionRefused` (idempotency_key, code, detail, stage IDEMPOTENCY | DEPTH | PARENT | WRITE_SET | CAPACITY | WORKTREE | LEASE | START, rolled_back), `SubagentCapsuleBound` (on the child: agent_id, parent_task_id, capsule_ref) and `SubagentResultRecorded` (agent_id, child_task_id, status, result_ref, summary, artifacts, evidence_refs, unresolved_risks, branch); harness tools `agent.spawn`, `agent.wait`, `agent.result`, `agent.cancel` (doc 17); `TaskOrigin` gains `subagent`; the harness refuses `WRITE_SCOPE_DENIED` (doc 14 "As built (M6.3, M6.5)"). All record-only. REQ-EV-0046: `SubagentProtectedEffect` on the parent (agent_id, child_task_id, idempotency_key, tool, effect_class, ceiling, call_id), followed by `TaskNeedsAttention` — a child's call above its capsule's effect ceiling was refused before the kernel (`PROTECTED_EFFECT_REFUSED`, no approval of its own) and the parent decides; record-only. REQ-EV-0115: `SubagentAdmitted.profile` and `narrowed_tools` (the profile compiled into the capsule and what it dropped); `SubagentAdmissionRefused.stage` gains `PROFILE`. REQ-EV-0180: `SubagentAdmitted.scheduling` (`SEPARABLE` | `BLOCKING` | `REQUESTED`) says why the mode in force is what it is. REQ-EV-0008: the harness tool `agent.attend` records `AgentNodeTransitioned` `BACKGROUND` ↔ `RUNNING` on the parent (reason "attention mode … by the parent"); nothing on the child's log. REQ-EV-0049: `SubagentResultRecorded.status` gains `PARKED` (an interim envelope, superseded by the child's final one); harness tools `agent.park` and `agent.resume` (doc 17). REQ-EV-0050 / 0179: `agent.steer` (doc 17); a new attempt's `SubagentResultRecorded.evidence_refs` carry `prior_result:<ref>` and `attempt:<n>`; on the child's log `TaskReturnedToWork` + `TaskWaiting` + `TaskInputQueued(FOLLOW_UP)` precede the new run. EPR-007: run event `ReviewLegActivated { plan_id, slot_id, activation, endpoint, model, candidate_revision, gate_ref, env_id, review_task_id, brief_ref, reserved_minor }`; task events `ReviewerResultRecorded { review_task_id, env_id, candidate_revision, verdict, confidence, result_ref, validated_findings, unsupported_findings, highest_severity, summary }`, `RevisionActivated { revision, max_revisions, result_ref, endpoint, model }`, `ReviewDecisionRecorded` with provenance `independent_reviewer`, `AcceptanceGateEvaluated` with trigger `INDEPENDENT_REVIEW`, `RouteReevaluated` at boundary `REVIEW`; harness tool `review.report` (doc 17, review tasks only); `RoutingPlanView.path_label` `CRITIQUE` / `CASCADE+CRITIQUE`. EPR-018: `AdmitReviewEnvironment { task_id, revision }` → `ReviewEnvironmentView { env_id, candidate_task_id, review_task_id, worktree, branch, revision, lease_id, sandbox }` (fenced; `SANDBOX_UNAVAILABLE` on a host without one); `DisposeReviewEnvironment { env_id, reason }` → `ReviewEnvironmentDisposedAck { env_id, killed, worktree_removed }`; task events `ReviewEnvironmentAdmitted` / `ReviewEnvironmentDisposed` on the candidate (record-only); `TaskOrigin` gains `review`; the exec protocol gains `ProbeSandbox` / `SandboxProbed` and `SessionInfo.cwd`. REQ-EV-0118: `GetPlan { task_id }` → `PlanView { current_version, versions: PlanVersionView { version, plan_ref, outcome, expected_files, verification, protected_effects, steps_json, provenance, reason, offset, annotations }, executed_versions }`; `RevisePlan { task_id, note, plan_json, provenance }` → `PlanRevisedAck { version, plan_ref, offset }` (fenced; refused `TASK_RUNNING` under a live loop, `BAD_PLAN` for an invalid edit); task event `PlanAnnotated { version, plan_ref, note, provenance }` (record-only); `TurnPrepared` gains `plan_version`. REQ-EV-0117: execution profile `plan` (doc 23). REQ-EV-0151 / 0275: `GetAttention { session_id }` → `AttentionView { items: AttentionItemView { kind, task_id, reference, reason, action, since_offset }, last_offset }` — read-only, derived from canonical unresolved state (doc 32 "As built (REQ-EV-0151, REQ-EV-0275)"). PX-005: `ApplyUserPatch { task_id, path, expected_workspace_revision, old, new, expected_file_revision, source }` → `UserPatchAppliedAck { workspace_revision, previous_revision, file_revision, before_hash, match_tier, offset, replayed }` (fenced; `task.author`; refused `STALE_REVISION` when the workspace or the file moved past what the client saw, `PROTECTED_PATH` / `PATH_OUTSIDE_ROOT` after symlink resolution, `NO_UNIQUE_MATCH` when `old` is not one place at a ladder tier, `TASK_RUNNING` under a live loop; a retry with the same `command_id` replays the record and writes nothing); task event `UserPatchApplied { path, before_hash, after_hash, workspace_revision, previous_revision, provenance: user_direct_edit, source, match_tier, command_id }` (record-only) and, in the same transaction, `FileChanged` on the workspace log with `provenance: user_direct_edit`, `op: user_direct_edit:edit` and the command id where a tool call id would be; `FileChanged` gains `provenance` (empty for the tool host). PX-001: `ResolveApproval.intent_hash` — when given it must equal the approval's or the decision is refused `INTENT_MISMATCH` with nothing recorded; every thin client sends the hash it showed (docs/29 "As built (PX-001)"). PX-004: `SubmitExternalDiagnostics { task_id, source, source_version, workspace_revision, diagnostics: ExternalDiagnostic { path, line_start, char_start, line_end, char_end, severity, code, message, file_revision } }` → `ExternalDiagnosticsAck { batch_ref, recorded, discarded, offset, replayed }` (fenced; `task.author`; refused `STALE_REVISION` for another workspace revision and `MALFORMED` before persistence); task events `ExternalDiagnosticsRecorded { source, source_version, workspace_revision, batch_ref, recorded, discarded, paths, provenance: external_ide }` and `ExternalDiagnosticsRejected { source, source_version, workspace_revision, code, detail }`, both record-only (docs/29 "As built (PX-004)"). PX-006: `ConfigureForge { forge, token, api_base_url, web_host }` → `ForgeConfigured { forge, api_base_url, web_host, token_held, egress }` (not journaled; `provider.configure`); task events `ForgePullRequestOpened { idempotency_key, owner, repo, number, url, head, head_sha, base, result }` and `ForgePullRequestUpdated { idempotency_key, owner, repo, number, url, state, result }` (record-only; the idempotency ledger). PX-007: `OpenPullRequest { task_id, expected_candidate_revision, base, title, remote }` and `UpdatePullRequest { task_id, expected_candidate_revision, remote }` → `PullRequestAck { status: APPROVAL_PENDING | OPENED | UPDATED | DENIED, approval_id, intent_hash, number, url, branch, head_sha, candidate_revision, effect_receipt_ids, replayed, detail }` (fenced; `review.decide`; `STALE_REVISION`, `NOT_REVIEWED`, `NO_FORGE`, `FORGE_HOST_MISMATCH`). PX-010: `CreateTask.issue_url` with origin `forge_issue` (`FORGE_ISSUE_UNREADABLE` and no task when the issue cannot be read); `TaskCreated.goal_text`; task event `TaskCreatedFromIssue { url, number, title, provenance: forge_issue, document_id }` beside the `ContextDocumentAttached` of the issue (record-only).

As built (M6.2): task events `CapacityTicketGranted` (ticket_id, holder, holds: ResourceVector, expires_at_ms, generation), `CapacityTicketReleased` (ticket_id, holder, reason RELEASED | LAPSED) and `CapacityDenied` (holder, needs, code, dimension, needed, available, live); command `GetCapacity` → `CapacityView` (limits, held, available, ttl_ms, tickets); `StartTask` refuses `CAPACITY_EXHAUSTED` / `EXCEEDS_POOL`; a task suspends `Waiting(Capacity)` with diagnosis `CAPACITY_LOST` when its lapsed ticket cannot be taken again (doc 14 "As built (M6.2)"). All record-only.

As built (EPR-006): run event `ContinuationActivated` (plan_id, from_plan_id, from_slot_id, slot_id, activation, trigger QUALITY_REJECTED, cause REPAIR_ESCALATED | NO_PROGRESS | RESUMED, endpoint, model, gate_ref, candidate_revision, reject_reasons, failed_leg_attempts, failed_leg_repair_attempts, spent_minor, reserved_minor, remaining_minor, currency, scale, note) beside the `SlotActivated` that admitted the continuation and a `RouteReevaluated` at boundary `QUALITY` (decision SWITCH; a STAY when nothing could activate, its reason the refusal). Record-only; a resumed run rebuilds the note into its transcript from it (doc 27 §9 "As built (EPR-006)").

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

As built (M8.3, `proto/modbit/v1/guest.proto`): the guest RPC is `GuestFrame` over the same length-prefixed framing — `GuestHello` (protocol major/minor, guest version, methods, boot id) from the guest first; `GuestAdmit` (the gateway's protocol, the ephemeral credential, the sandbox id, `GuestPolicy`) or `GuestRefused` (`PROTOCOL_UNSUPPORTED`, `METHOD_MISSING`, `BAD_FRAME`) from the gateway; `GuestAdmitted` back; then `GuestCall` (`call_id` as the nonce, `task_id`, `effect_id`, `capability`, `auth` = HMAC-SHA256 under the credential over the call with `auth` cleared, one of `GuestHealth`, `GuestExec`, `GuestReadFile`, `GuestWriteFile`, `GuestNetProbe`) answered by `GuestReply` (the call id, its own `auth`, one of the reports or `GuestRefusal`). Another protocol major on either side is refused before admission; the required methods are `health`, `exec`, `fs.read`, `fs.write`, `net.probe`. The gateway's worker-facing HTTP surface is `POST /v1/sandboxes`, `GET|DELETE /v1/sandboxes/{id}?tenant_id=`, `POST /v1/sandboxes/{id}/calls`, all under a worker bearer token.

As built (M8.5, guest protocol 1.1 — additive): `GuestCall` gains `GuestProcStart` → `GuestProcStarted{proc_id, pid}`, `GuestProcFollow{proc_id, after_cursor, max_bytes, wait_ms}` → `GuestProcOutput{data, cursor, truncated, running, exit_code, timed_out, cancelled, total_bytes}`, `GuestProcWrite`, `GuestProcCancel`, `GuestPtyResize` → `GuestProcAck`, and `GuestListDir` → `GuestDirListing`, `GuestStat` → `GuestStatResult`, `GuestMkdir` / `GuestRemove` / `GuestRename` → `GuestFsDone`; the capability a call names must cover its body (`proc.exec`, `pty`, `fs.read`, `fs.write`) or it is `BAD_CALL`. The gateway relays them as call kinds `proc.start`, `proc.follow` (output base64), `proc.write`, `proc.cancel`, `pty.resize`, `fs.list`, `fs.stat`, `fs.mkdir`, `fs.remove`, `fs.rename`, and adds `POST /v1/sandboxes/{id}/relink`. The local SurfaceProtocol gains `ConfigureSandboxGateway {base_url, worker_token, worker_id, tenant_id, lease_generation}` → `SandboxGatewayConfigured` (capability `sandbox.configure`, held by CLOUD_WORKER; not journaled — the token stays in memory). Task events: `SandboxLeaseAcquired {sandbox_id, backend, isolated, image_version, boot_id, workspace_root, lease_generation, egress[], credentials[]}` (M8.6 adds the admitted `host:port`s and the credentialed virtual hosts — never a secret), `SandboxReleased {sandbox_id, reason}`, `SandboxLost {sandbox_id, detail}` — no state change; the last two may follow a terminal state. `GuestPolicy.egress_proxy` (M8.6) tells the guest to run its local proxy; the provision body's `spec.egress[{host, port, capability}]` and `spec.credentials[{handle, virtual_host, target_url, header, value_prefix, capability, secret}]` carry the grants; `GET /v1/sandboxes/{id}/egress?tenant_id=` returns the broker's audit.

As built (REQ-EV-0043, ProtocolCapabilitySet): `HelloAck.client_capabilities` names what this connection's client kind may ask the Core for — separate from the execution authority a task carries in its leases and policy (doc 23). DESKTOP and IDE_ADAPTER hold the UI surfaces (`ui.selection`, `ui.code_view`) and the human decisions (`approval.resolve`, `question.answer`, `review.decide`) with `task.author`, `events.subscribe`, `session.control`, `attachments.ingest`, `repository.trust` (the desktop also `provider.configure`); CLI holds everything but the UI surfaces (a `SetTaskSelection` from it is `source: cli`); CLOUD_WORKER holds `task.author`, `events.subscribe`, `attachments.ingest` and, as built (M8.2), what it needs to act for the cloud's principals on its own Core — `session.control`, `approval.resolve`, `question.answer`, `review.decide`, `provider.configure`, `repository.trust` — plus `session.mirror` (`ImportMirroredEvents`, `ReadMirrorEvents`), which no other client kind holds; SANDBOX_GUEST holds `events.subscribe` only. A command outside the set is refused `CLIENT_CAPABILITY` at the transport before the Core looks at the task, and the task named stays valid — a task the CLI authored runs to its end and a desktop client works its UI surfaces. Test: `qual_ev_0043_a_headless_client_lacks_ui_only_capabilities_while_the_task_stays_valid`.

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
