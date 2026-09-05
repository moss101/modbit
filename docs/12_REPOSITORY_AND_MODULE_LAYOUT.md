# Clean Repository and Module Layout

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Monorepo

Use one clean monorepo for the product. Old repositories are donor/read-only; no subtree import of Code-OSS.

```text
modbit/
├─ Cargo.toml
├─ pnpm-workspace.yaml
├─ apps/
│  ├─ desktop/                  # Electron main, preload, React renderer
│  ├─ cloud-api/                # Rust HTTP/WSS API service
│  ├─ cloud-worker/             # remote Core host
│  ├─ sandbox-gateway/          # tenant-bound MicroVM substrate boundary
│  └─ cli/                      # headless thin client over SurfaceProtocol (Alpha)
├─ crates/
│  ├─ domain/                   # IDs, domain objects, state transitions
│  ├─ protocol/                 # local/cloud framing + generated schemas
│  ├─ core-runtime/             # scheduler, WorkGraph/AgentGraph/StateGraph, conditional plan admission
│  ├─ event-store/              # append-only event store + projections
│  ├─ protocol-state/           # pending calls/approvals/question/lifecycle
│  ├─ checkpoint/               # workspace/runtime checkpoint epochs
│  ├─ compaction/               # context history compaction epochs
│  ├─ memory/                   # governed engineering memory
│  ├─ effects/                  # receipts, hash chain, evidence refs
│  ├─ policy/                   # capabilities, approvals, protected paths, PolicyEnvelope, RealizedRisk
│  ├─ providers/                # model/embedding adapters, registry, profiler, conditional plan compiler
│  ├─ prompt-compiler/          # system/task/rules/skills/context assembly
│  ├─ skills/                   # skill manifests, loader, selector
│  ├─ tools/                    # typed registry + execution envelopes
│  ├─ procedural-runtime/       # embedded JS isolate + tools.* bindings
│  ├─ context/                  # Context Pack planner/packer/provenance
│  ├─ retrieval/                # exact/BM25/vector/AST/graph search
│  ├─ workspace/                # canonical file service + revisioning
│  ├─ git/                      # branches/worktrees/diff/commit
│  ├─ diagnostics/              # headless LSP lifecycle + normalized diagnostics
│  ├─ terminal/                 # exec client, streams, OutputRef handling
│  ├─ browser/                  # semantic browser protocol + control leases
│  ├─ sandbox/                  # execution router + sandbox gateway client
│  ├─ verification/             # deterministic gates, Acceptance Gate, test plans
│  ├─ secrets/                  # credential handles/broker interfaces
│  └─ observability/            # tracing, metrics, cost accounting
├─ services/
│  ├─ modbit-execd/             # durable PTY/process broker
│  └─ modbit-guest/             # sandbox guest RPC agent
├─ packages/
│  ├─ ui/                       # reusable React components
│  ├─ surface-protocol/         # TS generated protocol/API types
│  ├─ design-tokens/
│  ├─ ide-adapter-core/         # shared thin-client library and conformance suite for IDE adapters
│  └─ vscode-adapter/           # first IDE adapter (Beta); JetBrains only after conformance
├─ tests/
│  ├─ integration/
│  ├─ e2e-local/
│  ├─ e2e-cloud/
│  ├─ provider-live/
│  ├─ browser-live/
│  ├─ recovery/
│  ├─ security/
│  └─ fixtures/repos/           # real git fixture repositories
├─ benchmarks/
│  ├─ retrieval/
│  ├─ context-economics/
│  ├─ agent-engineering/      # Eval Harness, Policy Lab, Outcome Statistics materialization
│  └─ latency/
├─ docs/
│  ├─ decisions/
│  ├─ api/
│  ├─ operations/
│  └─ security/
└─ tools/
   ├─ architecture-lint/
   ├─ evidence-check/
   └─ release-gate/
```

## Dependency direction

`domain` depends on no infrastructure crate. `core-runtime` depends on domain interfaces; provider/tool/workspace/storage implementations plug in through explicit ports. UI protocol depends on domain DTOs, never database types. Sandbox and browser do not import scheduler internals.

Forbidden dependency examples:
- `retrieval -> desktop`
- `policy -> electron`
- `event-store -> provider implementation`
- `workspace -> browser`
- `guest -> cloud-api business logic`

Architecture CI runs `cargo metadata` and a dependency rule checker to reject forbidden edges.

## Ownership

- Core/runtime team: domain, state, scheduler, events, recovery.
- Context team: retrieval, prompt/skill compiler, memory.
- Execution team: tools, terminal, workspace, Git, sandbox, browser.
- Security/platform: policy, effects, secrets, cloud gateway.
- Surface team: desktop renderer/main/preload and SurfaceProtocol only.

A capability spanning teams still has one authoritative domain owner; cross-team behavior integrates through typed contracts rather than shared mutable tables.

## Execution policy implementation placement (DR-EPR-2026-09-05-v1.1)

No new crate, service, scheduler or storage authority is introduced. Every v1.1 component lives inside an existing crate under its existing owner; the ownership table in `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` and the task owners in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` are authoritative, this table says where the code goes.

| Component (contract in doc 38) | Owner | Crate / location | Delta tasks |
|---|---|---|---|
| Request Profiler, Model Registry read, conditional plan compiler, confidence-adjusted feasibility (LCB/posterior, mode tau/delta, cold-start priors, `QUALITY_FLOOR_INFEASIBLE`) | model-gateway | `crates/providers` routing modules | EPR-002/003/004/016 |
| `ConditionalExecutionPlan` schema-2 validator, slot admission and reservation, continuation activation, fenced restart | core-runtime | `crates/core-runtime` plan admission beside the scheduler; wire types in `crates/domain` and `crates/protocol`; durable slot/attempt projections in `event-store`/`protocol-state` | EPR-001/005/006/007/009/014 |
| `PolicyEnvelope` and factual `RealizedRisk` (required assurance; no learned risk score) | effects-security | `crates/policy`; receipts and evidence refs in `crates/effects` | EPR-008 |
| Isolated Non-Committing Reviewer environment: disposable review worktree, sandboxed review processes, deny-default egress/secrets, exact cleanup | effects-security (capability boundary), reusing workspace-git worktrees and the Execution Router | reviewer capability profile in `crates/policy`; review worktree via `crates/git` and `crates/workspace`; processes through `crates/sandbox`/`crates/terminal` under the `review_isolated` profile of `21_TERMINAL_EXECUTION_AND_SANDBOX.md`; tool projection per `16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` | EPR-018 |
| Acceptance Gate (`AcceptanceGateResult`, ACCEPT/REJECT/INCONCLUSIVE against required assurance) | verification | `crates/verification` | EPR-017 |
| Outcome Statistics Store (immutable `stats_version` snapshots) and offline Policy Lab | eval-bench | derived data materialized by the Eval Harness under `benchmarks/agent-engineering` into the existing `event-store`/artifact object store; the compiler reads it through a versioned read interface in `crates/providers`; no second database, memory or registry | EPR-011/012/013/015/019 |
| Request, leg and gate accounting; `RoutingDecisionRecord`/`OutcomeRecord` projections; DIRECT/CASCADE/CRITIQUE derived path labels | observability | `crates/observability`; events in `event-store` | EPR-010 |
| Role-specific context and skill compilation for solver, reviewer, reviser and escalation legs | context-engine, skills | `crates/context`, `crates/prompt-compiler`, `crates/skills` | inputs to EPR-005/007/013 |

Core calls the compiler through canonical routing interfaces, admits and executes exactly one bounded conditional transaction, calls the Gateway for inference and the normal tool runtime for effects. DIRECT, CASCADE and CRITIQUE are labels emitted by observability over the executed path, not modules or dispatch selectors. Providers cannot call workspace tools. Renderer, guest and plugin code receive no model credentials, policy weights or statistics. Legacy v1.0 `ExecutionPlan`, `QualityGateResult` and `CriticResult` exist only as decode adapters in `crates/domain`; new writers emit the v1.1 contracts.

## Client surfaces and forge placement (DR-PX-2026-09-05)

`apps/cli` and every IDE adapter are thin SurfaceProtocol clients owned by the desktop owner; they contain no orchestration, context, memory, Git, recovery, policy or tool code. The `forge.*` family lives in `crates/tools (external.*)` behind the External Tool Hub; pull-request semantics stay in `crates/git` and the Change Engine; CI evidence ingestion sits in `crates/verification`; webhook intake is a Cloud API endpoint. See `29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`.
