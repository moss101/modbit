# Core and Cloud Backend Implementation

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Rust runtime composition

`modbit-core` uses Tokio for async orchestration and explicit bounded channels. Every background worker has cancellation token, owner aggregate, capacity class and shutdown deadline. Unbounded `mpsc` channels are forbidden in event/tool/output paths.

## Startup sequence

1. Load config and installation identity.
2. Open/migrate SQLite with integrity checks.
3. Initialize object store and verify writable space.
4. Acquire Core singleton lock for user profile.
5. Start event/projection services.
6. Reconcile unfinished sessions/protocol state.
7. Start terminal broker client and diagnostics supervisors.
8. Initialize provider registry and health probes; verify the routing policy/registry bundle and pin the Outcome Statistics `stats_version` the compiler will read.
9. Initialize repository index manager.
10. Bind authenticated local IPC.
11. Emit `CoreReady` only after recovery completes.

## Shutdown

Stop accepting new mutating commands, checkpoint active tasks if policy allows, flush event store, detach rather than kill durable terminal/sandbox resources that should survive, revoke expired leases and persist final cursor. Forced shutdown path is separately chaos-tested.

## Scheduler

A single scheduler evaluates ready WorkGraph nodes and capacity tickets. It never directly executes privileged I/O; it issues typed commands to tool/execution components. Scheduler decisions are events so recovery can explain why a node started or waited.

## Session kernel lease

Each active session has one execution owner guarded by generation-fenced lease. Local mode uses DB/file lock; cloud mode uses Postgres lease with expiry/heartbeat. A stale owner can append audit events but cannot advance state after lease loss.

As built (M4.4): a run executes under the lease generation `StartTask` presented (`RunCreated`/`RunResumed.kernel_lease_generation`). Every state-advancing append of the agent loop — turns, steps, tool proposals and dispatches, checkpoints, compactions, outcomes — goes through `EventStore::append_fenced`, which checks the session's generation inside the append transaction and refuses with `StaleLease` when it moved; the loop also checks the lease before each turn, after each model response and before each tool call, and when the lease is gone it records `RunFenced` (the generation it held, the current one and its owner) and `RunSuspended`, and marks the task waiting for the new owner — those audit records are the only writes a stale owner makes. `StartTask` under the old generation is refused `STALE_LEASE`; the new owner resumes the run under its generation.

## Verification engine

Verification plan is materialized from task type, changed files, repository config and agent proposals. Deterministic steps include build/typecheck/lint/test/diagnostics/diff/security rules. Agent can propose extra checks but cannot delete mandatory policy checks. The Acceptance Gate is part of this engine: it materializes `AcceptanceGateResult` (ACCEPT/REJECT/INCONCLUSIVE, required_assurance, missing_evidence) against the assurance demanded by the policy-owned `RealizedRisk`, independently of plan success, and never weakens policy minima (ADR-R-052, EPR-017).

Execution of the plan follows `64_VERIFICATION_EXECUTION_CONTRACTS.md` (DR-PX-2026-09-05-006): a BASELINE run before the first write with `KNOWN_FAILING` labelling, bounded TARGETED runs whose selection is heuristic in M2 and impact-based from M3, a COMPLETION run at the final candidate revision, and regression attribution between them. Runner adapters in `crates/verification` normalize vitest/jest/mocha, pytest, cargo and go output into `TestReport`/`CheckResult` records with `STRUCTURED` or `HEURISTIC` parser confidence; `UNKNOWN` outcomes are `INDETERMINATE`, never passes. The engine owns the flake protocol (one isolated rerun of failed checks, `FLAKY` labelling and revision-scoped quarantine, never a skip list the agent can write) and evaluates the whole-diff invariants DI-1..DI-9 in the COMPLETION run, while the Change Engine evaluates the per-transaction invariants when a ChangeTransaction is proposed. `failure_signature` for the repair loop is derived only from normalized `CheckResult`s.

## Cloud API implementation

Rust HTTP service with structured auth middleware, request IDs, tenant context and rate limits. It writes commands/events to Postgres and streams committed events. It does not run model/tool code in request handlers.

## Cloud worker lifecycle

Worker claims ready session lease, loads projection/protocol state, executes Core runtime, renews lease, writes heartbeat/metrics and releases lease on safe shutdown. If heartbeat expires, another worker waits fencing grace period and then resumes with a higher generation.

## Sandbox Gateway

Gateway validates bearer/mTLS worker identity + tenant/session/sandbox lease. It compiles capability into MicroVM substrate network/mount/resource policy, starts `modbit-guest`, exchanges ephemeral guest credential and returns a sandbox handle. All guest calls are signed/nonce-protected or carried on mutually authenticated channel.

## Provider requests

Provider Gateway records request metadata before network dispatch and model usage after completion. Raw hidden reasoning is not persisted unless explicitly exposed by provider and permitted; normal model messages/tool calls are stored as protocol/evidence according to retention policy.

## Backpressure

- Model deltas: coalesce small text fragments before UI event publication.
- Terminal/browser streams: cursor + bounded ring + OutputRef spill.
- Tool payloads: inline size ceiling; larger objects by ref.
- Event stream: per-client bounded queue; slow clients reconnect/replay rather than block Core.

## Idempotency

All mutating commands and tool dispatches have stable IDs. Cloud API stores command result or accepted event pointer. Retried HTTP requests never create duplicate tasks/approvals/effects.

## Conditional plan compiler and bounded transaction executor wiring

During provider initialization verify last-known-good registry/policy bundle signatures, freshness and schema compatibility, and pin the Outcome Statistics `stats_version` the compiler will join; local Core can use a fresh cached bundle. Reconcile the active `ConditionalExecutionPlan`, its slot table, attempt, budget reservation and routing epoch alongside ordinary unfinished protocol state before `CoreReady`. The model-gateway owner compiles plans and applies the confidence-adjusted quality floor; Core's plan admission validates every continuation slot and its worst-case reservation before the first dispatch, and only Core's existing scheduler dispatches the initial leg and activates precompiled slots. The Policy Kernel supplies the `PolicyEnvelope` and derives factual `RealizedRisk` from the candidate revision; the Verification Engine materializes the independent `AcceptanceGateResult`; Context/Prompt/Skill compilers prepare each role; the Gateway owns provider secrets and per-attempt usage; observability records request, leg and gate outcomes separately so a successful escalation never credits a failed initial leg.

Before each leg, revalidate current entitlements/lease/epoch and reserve finite remaining resources. Cancellation propagates to the Gateway, to `review_isolated` processes and worktrees, and to normal tool cancellation domains; it persists partial evidence/accounting and reconciles unknown effects. Recovery never retries an ambiguous protected effect merely because the initial leg failed. A stronger realized-risk obligation suspends acceptance until an already-compiled slot and verification can satisfy it; a missing slot stops safely rather than synthesizing a branch. Algorithms and typed failures are normative in `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`.
