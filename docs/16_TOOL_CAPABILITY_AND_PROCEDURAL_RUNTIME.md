# Tool System, Capability Kernel, Procedural Runtime, and MCP

> **Authority date:** 2026-09-03  
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
