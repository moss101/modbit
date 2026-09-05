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
│  └─ sandbox-gateway/          # tenant-bound MicroVM substrate boundary
├─ crates/
│  ├─ domain/                   # IDs, domain objects, state transitions
│  ├─ protocol/                 # local/cloud framing + generated schemas
│  ├─ core-runtime/             # scheduler, WorkGraph/AgentGraph/StateGraph
│  ├─ event-store/              # append-only event store + projections
│  ├─ protocol-state/           # pending calls/approvals/question/lifecycle
│  ├─ checkpoint/               # workspace/runtime checkpoint epochs
│  ├─ compaction/               # context history compaction epochs
│  ├─ memory/                   # governed engineering memory
│  ├─ effects/                  # receipts, hash chain, evidence refs
│  ├─ policy/                   # capabilities, approvals, protected paths
│  ├─ providers/                # normalized model/embedding adapters
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
│  ├─ verification/             # deterministic gates/test plans
│  ├─ secrets/                  # credential handles/broker interfaces
│  └─ observability/            # tracing, metrics, cost accounting
├─ services/
│  ├─ modbit-execd/             # durable PTY/process broker
│  └─ modbit-guest/             # sandbox guest RPC agent
├─ packages/
│  ├─ ui/                       # reusable React components
│  ├─ surface-protocol/         # TS generated protocol/API types
│  └─ design-tokens/
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
│  ├─ agent-engineering/
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

## EPR implementation placement (DR-EPR-2026-09-05)

Extend `crates/providers` with model registry, Request Profiler and deterministic execution plan compiler modules behind the existing model-gateway owner. Keep bounded leg transitions/budget reservations/routing epochs in `crates/core-runtime`, wire types in `crates/domain` and `crates/protocol`, durable projections in existing event/protocol stores, risk in `crates/policy`, gates in `crates/verification` and accounting in `crates/observability`. The Policy Lab is a workflow of the existing Eval Harness/benchmarks, not another runtime, memory or policy authority.

Core calls the compiler through canonical routing interfaces, then calls the Gateway for inference and the normal tool runtime for effects. Providers cannot call workspace tools. Context/Skill owners provide role-specific inputs; renderer/guest/plugin code receives no model credentials or policy weights. See the ownership table in `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` and task owners in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`.
