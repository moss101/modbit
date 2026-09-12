# Tool System, Capability Kernel, Procedural Runtime, and MCP

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Tool registry

Every tool is registered with immutable versioned metadata:

```text
ToolSpec {
  namespace, name, version
  input_schema, output_schema
  effect_class
  required_capabilities[]
  execution_profiles[]
  timeout_policy
  output_budget
  idempotency_semantics
}
```

Core namespaces include `fs`, `git`, `shell`, `search`, `diagnostics`, `test`, `browser`, `artifact`, `memory`, `workspace`, `cloud` and `external.*`.

## Effect classes

`READ_ONLY`, `REVERSIBLE_WRITE`, `PROTECTED_WRITE`, `EXTERNAL_SIDE_EFFECT`, `SECRET_ACCESS`, `DESTRUCTIVE`. Policy can elevate a specific path/domain/tool above its default.

## Dynamic task-scoped projection

Model never receives the entire registry. Prompt Compiler projects only tools authorized and likely useful for the active node, including capability explanation and effect class. Projection has a version/hash recorded in the Turn.

Projection is per leg role of the active `ConditionalExecutionPlan`. An Isolated Non-Committing Reviewer leg (ADR-R-053, EPR-018) is projected only tools that are `READ_ONLY` on canonical state plus `REVERSIBLE_WRITE` and process execution scoped to its disposable review worktree under the `review_isolated` execution profile of `21_TERMINAL_EXECUTION_AND_SANDBOX.md`. `PROTECTED_WRITE`, `EXTERNAL_SIDE_EFFECT`, `SECRET_ACCESS`, `DESTRUCTIVE`, Git commit/push and deploy tools are excluded from the reviewer projection and are additionally denied by the Capability Kernel if requested, so the projection is a convenience and the kernel is the boundary. Reviewer context excludes the solver's hidden reasoning by default; reviewer tool cost and latency are part of plan pricing. Solver, reviser and escalation legs use the ordinary task-scoped projection.

### Projection as built (M5.1)

The projection is compiled every turn from three layers, each narrower than the last: the host list (what the Tool Registry supports under the task's execution profile, its capability lease and the kernel), the deferred catalog (REQ-EV-0134: deferred toolsets are named on `tool.search` and hydrated once activated by search or by a direct call), and the **node scope** derived from the harness plan for the leg role. Under the scope: `READ_ONLY` tools are always projected; a `REVERSIBLE_WRITE` tool that writes files (`fs.write`) is projected once the plan names `expected_files` (`DECLARE_WRITES`); a `PROTECTED_WRITE`, `EXTERNAL_SIDE_EFFECT`, `SECRET_ACCESS` or `DESTRUCTIVE` tool is projected once the plan names it, its toolset or its capability in `protected_effects` (`DECLARE_PROTECTED_EFFECT`) — and still needs its approval at dispatch; process execution is not a file write and is projected with the read-only set. A reviewer leg is projected read-only tools and reversible writes for its disposable worktree and never a protected class (`REVIEWER_LEG`). What is withheld is told to the model on `plan.update` with the reason and what unlocks it, so a refusal is never the first the model hears of it.

Every turn's `ToolProjectionSelected` records the projection hash, the projected names, the withheld names with their reasons and the leg role. A call to a tool outside the turn's projection — a crafted name, a tool the scope withholds, a deferred tool the scope would withhold, a tool the plan declared in this very turn — is refused at the pipeline's policy stage before any effector (`POLICY_DENIED` / `TOOL_NOT_PROJECTED`: Proposed → Validated → PolicyDecision denied, which ends the call; never Dispatched) with the same way forward; the harness gates of doc 14 (plan, scope, retrieval, repair) still answer first for the tools they govern, so a write before any plan is `PLAN_REQUIRED` as before; a direct client call carries no projection and is judged by the kernel alone. The kernel remains the boundary for everything that was offered. A scope change is a projection change and therefore a prompt-cache miss (the projection hash is in the cache key): the model pays it once, when the plan first declares files or effects. Proof: `qual_m5_1_projection_follows_the_plan_and_refuses_crafted_calls` (E2E-011) and the `projection_follows_the_plan_and_the_leg_role` rule test.

## Procedural Tool Runtime

Validated procedural-runtime evidence is adapted into a Modbit-owned P0 runtime. For eligible tasks the model-visible surface can be reduced to:

```text
exec(program, declared_effects, budget)
wait(handle, timeout)
request_user_input(question, schema)
```

`exec` runs JavaScript in an embedded **QuickJS isolate** with no network, filesystem, process or dynamic-module access. The only host bindings are capability-filtered `tools.*` async functions generated from ToolSpecs. CPU instruction/time, memory, call count and output budgets are enforced by the host.

Example conceptual program:

```javascript
const hits = await tools.search.symbol({name: "SessionStore"});
const file = await tools.fs.read({path: hits[0].path});
const patch = buildPatch(file.text);
await tools.fs.apply_patch({path: hits[0].path, patch});
const test = await tools.test.run({target: "session-store"});
return {test};
```

The isolate cannot bypass the Capability Kernel: each binding is a normal tool call with ToolCallId, policy check and receipt.

## Direct mode

Models/tasks that are more reliable with native function calling use direct typed tools. Direct and procedural mode share the same registry and effects; they are not separate harnesses.

## Skills

Skill packs contain manifest, compatibility range, instructions, optional procedure templates, eval metadata and provenance. Selection can be explicit or Context/Skill selector driven. Skills compile into minimal instructions + tool projection; large skill content is loaded by reference when needed rather than blindly injected every turn.

Skill lifecycle: `incubator → evaluated → signed → enabled`. A skill cannot request capabilities beyond task/user policy.

## MCP / external tools

External tool gateway follows dynamic `list → call → cancel` semantics with `session_id/task_id/turn_id/call_id`. Discovered tool schemas are namespaced `external.<server>.*`, size-bounded and treated as untrusted. MCP server content cannot inject system instructions or new capabilities. Cancellation and unknown-outcome reconciliation are mandatory for effectful calls.

## Large results and OutputRef

Any result beyond inline budget is persisted as immutable OutputRef with mime/type, byte length, checksum, preview and paginated/ranged reads. Model gets concise metadata + selected slices. This prevents terminal/search/browser output from exploding context.

## Tool completion proof

Each tool has three test levels: schema/contract, real integration against its actual substrate, and Agent Runtime E2E. Tool code that exists but is not reachable through the Registry + policy + event loop is **not implemented** for completion accounting.


## V2 canonical inventory and source parity

`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` is normative for tool ownership. External-reference capability names are compatibility/provenance only. Every executable capability still passes ToolCallNormalizer → Capability Kernel → effector → typed result → Evidence/Effect Ledger. Rich tool media is normalized through Media Pipeline, and discovery/skill activation can never authorize execution.
