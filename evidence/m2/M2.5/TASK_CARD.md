# Task Card — M2.5 Capability Kernel + basic approval flow

## Identity

- Task ID: M2.5
- Milestone: M2
- Canonical owner: effects-security (`crates/policy` is THE doc 81 policy kernel and approval/effect dispatch; the Core binds it per call)
- Requirement IDs: REQ-EV-0080 (policy before execution: host authorization is the boundary), REQ-EV-0045 (autonomous mode is a bounded profile, never above its ceiling), REQ-EV-0091 (admin policy precedence), REQ-EV-0093 (workspace trust ≠ sandbox grants); docs/23 "Capability kernel", "Approval policy", "Protected-effect receipt chain", "Emergency stop"; docs/43 M2.5.
- Risk class: high (authorization boundary for real effects)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Every tool call crosses one host boundary before any effect. A `CapabilityLease` granted at task creation (tenant/task, resource selectors, operation set, effect ceiling, execution profile, generation) is the authority; the admin `PolicyEnvelope` caps effect class per profile and denies capabilities that no lower authority can widen; the resolved configuration's DENY/ASK decisions apply; protected/external/secret/destructive effects wait for an approval bound to the normalized intent hash + scope + expiry. Executed protected effects append a hash-chained `EffectReceipt`. An emergency stop revokes every lease in the session and blocks new effects.

## Non-goals

- Fine-grained resource selector matching (path/domain globs on the lease; the kernel checks operation ids and ceilings, the workspace policy checks paths).
- Loading admin/project/user configuration layers into the Core (the kernel consumes a `ResolvedConfig`; the Core passes none yet).
- Approval UI in the desktop app; the agent runtime waiting on approvals (M2.7).
- ProtocolCapabilitySet negotiation (IMP-EV-0043/0044) and UI risk classification (IMP-EV-0088).

## Required reading

`AGENTS.md`, `docs/16`, `docs/21`, `docs/23`, `docs/30`, `docs/31`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (the M2.4 pipeline had a kernel port with a lease-less default policy; no leases, approvals, receipts or stop)
- production entry point: `modbit_policy::kernel::CapabilityKernel::decide`, `modbit_policy::ledger`, Core `InvokeTool` (kernel bound per call), `ListApprovals`, `ResolveApproval`, `EmergencyStop`, `GetEffectReceipts`, `GetCapabilityLeases`; CLI `approval list/resolve`, `stop`, `receipts`, `lease list`
- real effector/storage boundary: event store schema v5 (`approvals`, `capability_leases`, `effect_receipts`, `sessions.emergency_stopped_at`, `tool_calls.approval_id`) derived from `CapabilityLeaseGranted/Revoked`, `ApprovalRequested/Resolved`, `ToolCallApprovalRequested`, `EffectReceiptAppended`, `EmergencyStopActivated`

## Invariants

- No lease, no effect: a tool name is never authority; a revoked, expired or wrong-profile lease is a typed deny.
- Decisions only tighten: envelope → lease ceiling → configuration → approval; no later step widens an earlier deny.
- An approval authorizes exactly one intent hash before its expiry; the same `tool_call_id` with different arguments is refused (`TOOL_CALL_ID_REUSED`).
- Unattended profiles (`local_autonomous`) cannot exceed `REVERSIBLE_WRITE` and cannot ask.
- Protected effects that execute append a receipt whose hash recomputes and links to its predecessor; `GetEffectReceipts` verifies the whole chain.
- Emergency stop revokes every active lease in the session and blocks new effects; reads that need a lease are refused too.

## Verification

- Kernel (crates/policy/tests/kernel.rs): lease authority; QUAL-EV-0045 autonomous ceiling and no-ask; QUAL-EV-0091 project allow cannot weaken admin deny; QUAL-EV-0093 trusted workspace still denied network/secret; approval intent/expiry binding; emergency stop blocks effects not reads
- Ledger unit test: chain verifies; tampering and a missing predecessor are detected
- Pipeline (crates/tools): destructive `git.worktree.close` is `APPROVAL_PENDING` with no effect; a bound kernel adapter executes it
- Core: `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals` over the real socket: lease granted at task creation, approval opened/listed/resolved, replay and intent mismatch, receipt chain valid, denial is final, autonomous profile ceiling, emergency stop revokes leases and blocks writes, event trail
- CLI smoke: lease list, approval list/resolve, receipts, stop
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
