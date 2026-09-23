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

### Isolate as built (M5.2)

`crates/procedural-runtime` runs a program as the body of an async function in a fresh quickjs-ng runtime (`rquickjs` 0.13, doc 35) on a dedicated thread with its own single-threaded executor. The runtime has the engine's core intrinsics and nothing else: no `std`/`os` library, no module loader (`import()` rejects), no `require`, `process`, `fetch`, `WebSocket`, `Worker`, timers or dynamic loading (`DENIED_GLOBALS` names what the isolate's own test proves absent). The only host bindings are `tools.<toolset>.<name>(args)` — async functions generated from the names the `Host` offers (the node's projection), each of which hands the name and the JSON arguments to the host and resolves with the host's JSON or throws an `Error` carrying the host's `code` (`POLICY_DENIED`, `TOOL_NOT_PROJECTED`, …); the raw binding is removed from the global object after the prelude and the `tools` object is frozen. `console.log` writes to a bounded log. Budgets are the host side of the engine: a CPU deadline enforced from the interpreter's interrupt handler (with an optional ceiling on interrupt polls as an instruction-proportional measure), a heap ceiling, a stack ceiling, a `tools.*` call ceiling (the call past it never reaches the host), a return-value byte ceiling and a log byte ceiling; an exhausted budget ends the program with the budget named (`BudgetExhausted { CPU_TIME | MEMORY | STACK | TOOL_CALLS | OUTPUT }`). The outcome carries the returned value as JSON, the error with its constructor name and stack, the log, every binding call in order with its outcome, the interrupt polls, the elapsed time and the peak heap. Nothing in the isolate authorizes anything: the `Host` is the seam through which M5.3 routes every binding through the Tool Registry and the Capability Kernel with a ToolCallId, a policy decision and a receipt.

### Bindings and the model-visible surface as built (M5.3 / M5.4)

`services/modbit-core/src/procedural.rs` is the host behind every program: its bindings are the turn's projection (M5.1, registry tools only — the harness's own tools stay the model's) narrowed by the effects the program declared (`declared_effects`: tool names or toolsets), and every binding call is served through the same pipeline as a direct model call — a fresh `ToolCallId`, the projection fence, the Capability Kernel, the approval flow (the task waits on the approval; the program resumes with the decision, `APPROVAL_DENIED` and no effect when the user says no), the receipt — one tool-call aggregate per call with the exec call's id as its `call_id` prefix (`<exec>#<n>`). The harness gates of doc 14 that govern writes are applied by the host from the harness state the program started under: `HARNESS_PLAN_REQUIRED`, `HARNESS_PLAN_REVISION_REQUIRED`, `HARNESS_REPAIR_ATTEMPT_REQUIRED`; a scope question cannot be asked from inside a program and refuses the write instead (`HARNESS_SCOPE_QUESTION_REQUIRED`), so the model asks outside it. A program that may write meets the BASELINE-before-first-write rule of doc 64 like a direct write.

The model-visible surface is `proc.exec(program, declared_effects, budget)` and `proc.wait(handle, timeout_ms)`; `request_user_input` is the existing `user.ask`, the model's to call between programs. `proc.exec` returns the outcome when the program ends within a short grace (2 s) and otherwise a handle (`RUNNING`, typically a program awaiting an approval); `proc.wait` returns the outcome once it ends, or `RUNNING` again after its timeout (60 s at most); a finished handle answers again with the same outcome; `task.complete` is refused while a program runs (`PROGRAM_RUNNING`), and a program still running when the loop ends is cancelled. Budgets are the runtime's defaults capped by ceilings (CPU 120 s, heap 256 MiB, output 1 MiB) and by the task's remaining tool budget; a program's calls count against it. The outcome the model sees is bounded (status, handle, error, the calls with their outcomes, the log, the value up to 16 KiB); the full outcome is a content-addressed object named in it. The log carries `ProgramStarted { handle, program_ref, declared_effects, bindings, budget }` and `ProgramEnded { handle, status, budget_exhausted, outcome_ref, tool_calls, elapsed_ms, interrupt_polls }` on the task, and a `ProcedureRun` step per exec/wait. After a program ends its writes are the task's writes (scope counters, candidate revision), its failed checks are the task's open failures, and its calls are the task's tool calls. Proof: `qual_m5_e2e_012_a_program_composes_governed_tools_in_the_isolate` (E2E-012).

### Tool-schema and token economics as measured (M5.6)

`qual_m5_6_direct_and_procedural_modes_yield_the_same_effects_at_different_token_costs` pairs the same coding task (read `a.txt`, create `b.txt` from it, run a shell command, complete) three times through the real Core in direct mode (one governed call per model turn) and in procedural mode (one `proc.exec` program composing the same three calls), and publishes a paired report (`benchmarks/context-economics`: `model_calls`, `tool_schema_bytes` — the bytes of tool schemas in every request, summed — `input_tokens`, `tool_calls`, with the method stated). Effect and policy parity holds in every pair: the same three tool calls with the same outcomes on the log, each through the same pipeline, the same file. At the deterministic local model server, procedural mode makes 3 model calls to direct mode's 5 (4 when the program outlives `proc.exec`'s 2 s inline grace, as on a slow runner, and the model waits once), sends 41.7 KB of tool schemas to 70.8 KB (−41 %) and 3.1k prompt tokens to 5.9k (−46 %); the intervals have no width because the product is deterministic here, and whether a live model would write the program correctly or prefer the mode is not established by this benchmark. The eager-all-tools baseline of QUAL-EV-0116 (projected schema bytes against the whole host list) remains in `qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy`.

## Direct mode

Models/tasks that are more reliable with native function calling use direct typed tools. Direct and procedural mode share the same registry and effects; they are not separate harnesses.

## Skills

Skill packs contain manifest, compatibility range, instructions, optional procedure templates, eval metadata and provenance. Selection can be explicit or Context/Skill selector driven. Skills compile into minimal instructions + tool projection; large skill content is loaded by reference when needed rather than blindly injected every turn.

Skill lifecycle: `incubator → evaluated → signed → enabled`. A skill cannot request capabilities beyond task/user policy.

### Skills as built (M5.5)

`crates/skills` owns the package format, the registry, the selector, the compiler and the signing check; `services/modbit-core/src/skills.rs` runs them for a task. A package is a directory with a `SKILL.md` whose front matter is the manifest — `name`, `version`, `description`, `compatibility.modbit`/`compatibility.models`, `required_tools` (names or toolsets), `capability_ceiling`, `model_invocable`, `triggers`, `provenance.*`, `eval.*` — and whose body is the instructions; `procedures/*.js` are templates for `proc.exec`; `resources/*` are loaded by reference, never injected. Its identity is the content hash over every file except the attestations. The lifecycle is decided by what sits beside the manifest and by policy: nothing → `INCUBATOR`; an `EVALUATION.json` of this exact content with disposition `PROMOTE` → `EVALUATED`; a `SIGNATURE.json` whose Ed25519 attestation (name, version, content hash) verifies under a trusted key (`MODBIT_SKILL_KEYS="<id>:<hex>,…"`, the model registry's convention) → `SIGNED`; `ENABLED` is policy's word — signed skills are enabled, unsigned ones only under `MODBIT_SKILLS_ENABLE_INCUBATOR=1` (development). A package changed after signing is `INCUBATOR` again with the refusal in its note; a signature by an untrusted key never counts.

A run discovers packages under `<workspace>/.modbit/skills` (project skills, shadowing by name) and `<profile>/skills`, selects the names `StartTask.skills` carries (`--skill` on the CLI) — refused with the reason when unknown or not enabled — and every enabled, model-invocable skill one of whose trigger phrases occurs in the goal, compiles each against the task's policy surface (profile × lease × kernel: the skill's `required_tools` intersected with it; what the policy does not offer is told to the model as unavailable, never granted; the instructions bounded to 8 KiB with the cut declared; procedure templates named), injects the instructions into the rules segment of every request (a stable prompt-cache segment) and records `SkillSelected { name, version, content_hash, lifecycle, source, reason (EXPLICIT | TRIGGER:<phrase>), instructions_hash, instructions_truncated, tool_projection, tools_unavailable }` and `SkillRejected { name, code, reason }` on the task — the instruction manifest of REQ-EV-0105 for skills. The node's turn-by-turn projection (M5.1) is untouched by a skill. Proof: `qual_m5_5_signed_skills_are_selected_compiled_and_recorded_and_unsigned_ones_are_not` (real Core) and the package tests in `crates/skills/tests/packages.rs`.

A package larger than 4 MiB, or with a file over 1 MiB, is refused (`OVERSIZED`); a package that fails to load is recorded on the task as `SkillRejected` by its directory with the parser's code and reason, so an invalid skill on disk is visible, not silent. The lint of REQ-EV-0230 sits in the tool matrix: every registered tool namespace names the docs/17 inventory row it realizes and its build/buy justification (`qual_ev_0230_…`). Further proofs: `qual_ev_0061_0214_…` (a hostile package cannot widen the task's authority; a `model_invocable: false` skill is never selected by trigger), `qual_ev_0105_0114_…` (the skill segment survives a compaction epoch; the registry follows the disk), `qual_ev_0209_…` (malformed and oversized packages refused), `qual_ev_0213_…` (the compiled projection against an eager injection).

Extension-provided packages (REQ-EV-0181, REQ-EV-0210) are installed by the client — `modbit-cli skill install <dir> [--expect-hash <hex>] [--replace]`, `skill remove <name>`, `skill list` — into `<profile>/skills/<name>`: the package must load and, when an expectation is given, its content hash must match; a `PROVENANCE.json` (source, hash, expectation, time; outside the content identity) is written beside it; installation grants nothing — the registry decides the lifecycle from the shipped attestations and policy, and a run compiles the skill against its policy surface as any other. Proof: `qual_ev_0181_0210_an_extension_skill_installs_lists_runs_its_procedure_and_survives_removal_and_reload` (apps/cli against the real Core).

### Rules as built (REQ-EV-0059 / 0105 / 0129)

Workspace and user rules are Markdown files under the workspace's `.modbit/rules/` (project layer) and the profile's `rules/` (user layer), each with optional front matter — `id` (the file stem by default), `paths` (globs; absent = global), `priority`, `expires_at_ms` — parsed by `crates/prompt-compiler::rules`. They are loaded once per run and selected every turn from the paths the task has made active (read or retrieved through the context ledger, written, or planned): a scoped rule stays dormant until a matching path is active and enters the prompt with the path and glob that activated it; precedence is deterministic — project over user, then priority, then file name — and a rule that loses to another with the same id is a recorded conflict naming the winner, both sources and what decided it; an expired rule is excluded and named; a file that is not a rule is reported. The active rules enter the rules segment of the prompt headed `[rule <id> — <layer>]`, and every change of the selection is a `RulesSelected` on the task (active with layer, source, hash and reason; dormant; expired; conflicts; invalid) — the instruction manifest of REQ-EV-0105 for rules. Proof: `qual_ev_0059_0129_scoped_rules_activate_lazily_and_conflicts_name_the_winner` (real Core) and the rules unit tests.

## MCP / external tools

External tool gateway follows dynamic `list → call → cancel` semantics with `session_id/task_id/turn_id/call_id`. Discovered tool schemas are namespaced `external.<server>.*`, size-bounded and treated as untrusted. MCP server content cannot inject system instructions or new capabilities. Cancellation and unknown-outcome reconciliation are mandatory for effectful calls.

### As built (M9.4: REQ-EV-0104 lifecycle, REQ-EV-0193 transport pool)

The gateway is `crates/mcp` (pure: JSON-RPC 2.0 framing over newline-delimited stdio, the `initialize` handshake, the bounded parse of an untrusted declaration and of a result's content blocks, the configuration fingerprint, and the in-flight call table) and the External Tool Hub in `services/modbit-core/src/mcp.rs` (the child processes, their pipes, the pool and the credentials). An agent sees exactly three tools — `external.list`, `external.call`, `external.cancel` (`crates/tools/src/external.rs`) — and they pass the same registry, kernel and pipeline as every other tool.

**A server is a program, not an authority.** A discovered tool's name is validated and namespaced `external.<server>.<tool>` (a dot or whitespace in the name, or a name that would take a native tool's, is refused by name while the rest of the list stands); its description is control-stripped and truncated; its schema must be a JSON-Schema object inside a byte and depth bound with no `$ref`; and every field the host does not know — a `requiredCapabilities`, a `systemPrompt`, an `effectClass` — is dropped before the host looks at the tool. A server declaring more tools than the bound is truncated and the drop recorded. Arguments are checked against the server's own declared schema before anything is sent.

**Only the host decides what a call costs.** `external.call` is registered at a floor of `ReadOnly` and `Tool::effect_of` raises every call to `ExternalSideEffect` unless the *host's* configuration declared that tool a read (`crates/mcp::ReadDeclarations`); an unknown tool or an unconfigured server is never a read, a server's own `readOnlyHint` changes nothing, and a server's `destructiveHint` takes the host's read declaration away. The judged class — not the floor — is what `ToolCallProposed` records, so the log and every projection over it carry the real class. An external side effect is therefore approval-bound by the kernel under the normal policy, and `external.call` is excluded from the reviewer projection and absent from the `review_isolated` lease (docs/17).

**Discovery is lazy and transports are pooled (REQ-EV-0193).** A transport is keyed by `(tenant, workspace root, configuration fingerprint)`: two sessions of one tenant in one workspace under one configuration share a single server process — same pool key, same pid, `sharers` above one, one handshake — while a different tenant, a different workspace or one changed byte of configuration is a different key and a different process. A trusted server is started the first time something lists or calls it; a server that is only *proposed* is never started (REQ-EV-0224). A dead transport is never handed out again.

**Cancellation and unknown outcomes.** Every request is registered before it is sent and holds a guard: a cancellation, a timeout, a dropped task or a dead server all send `notifications/cancelled` naming the request id and then reconcile. A read that ends without an answer failed; an effectful call that ends without an answer is reported `EXTERNAL_OUTCOME_UNKNOWN` — the host never claims the effect did not happen (docs/54 fault 25).

**Credentials.** A server's credential is a handle name in the configuration; the value is taken into the Core's memory at boot, placed into the child's environment at spawn, and appears nowhere else — and it joins the Core's custody set, so a tool call carrying its value is refused before policy like any other secret (M9.3). A configuration that tries to carry a secret-shaped environment entry is refused `SECRET_IN_CONFIG`.

**Which servers a task sees, and what it may reach (M9.4: REQ-EV-0128).** The servers are not the hub's own state: they are the task's **resolved configuration** — the admin, project and user layers of `modbit_policy::config` (REQ-EV-0039) read from `MODBIT_ADMIN_CONFIG` or `<data-dir>/admin-config.json`, `<workspace>/.modbit/config.json` and `<data-dir>/config.json`, resolved once per task and pinned for its life. The merge laws are the resolver's: a higher layer's deny of a name always wins, a lower layer may add a server nobody denied, and a name two layers both define is decided by the higher one with the lower layer's definition kept in the provenance rather than dropped in silence. `external.list` reports, per server, which layer decided, whose definition was overridden, and — for the servers that are not there — which layer's addition or removal the resolver refused. A file that does not parse is no opinion, reported on stderr: a broken configuration narrows nothing and stops nothing. A layer may also carry `review_comment_authors` (PX-008): the forge identities whose pull-request comments may steer a task, granted by the admin layer only and narrowed below it (doc 29 "As built (PX-008)").

A server declares the host capabilities it needs (`requires`, plus `secret.use` implicitly whenever it names a credential), and those are checked against the **task's own capability lease** before the server is started: a task that could not use a secret itself cannot have a server use one for it. An unleased server is `UNLEASED` in the listing with what is missing, and a call to it is refused `EXTERNAL_CAPABILITY_NOT_LEASED` before any process is spawned. A server's `scopes` are labels in the server's own vocabulary — audit and display, never authority.

A pooled transport is handed out only while it still answers: reuse pings it first, and a server that has stopped answering is replaced rather than reported ready.

**Proposing a server, and trusting one (M9.4: REQ-EV-0224).** A client — on its own behalf, or relaying what an agent suggested — proposes a server with `ProposeExternalServer`. The host validates the definition, refuses a name a higher configuration layer has denied (at propose time, rather than silently at resolve time), and writes it into the **user configuration layer** as `PROPOSED`: the same file the resolver reads and a person edits, so there is no second store of external servers to disagree with the configuration. A proposal is inert — `UNTRUSTED` in the listing with no tools, `EXTERNAL_SERVER_UNTRUSTED` on a call, and its process is never spawned. Proposing is not installing.

`TrustExternalServer` is a person's decision and is held to the same client capability as trusting a repository (`repository.trust`): letting a program the host did not write run against this workspace is that kind of decision. Every gate is answered there — the definition must still validate, no higher layer may deny the name, and a named credential must already be in the Core's custody (`ConfigureExternalCredential`, which is not journaled and answers with everything but the value). Taking trust away stops the program, not merely the next task's access to it. Because a task's configuration is pinned, the task that saw a proposal keeps seeing it untrusted; the next task sees the server ready. Proof: `qual_ev_0224_a_proposed_external_server_is_inert_until_the_host_trusts_it_and_holds_its_credential`.

**Media a server returns (M9.4: REQ-EV-0187).** An `image` or `audio` block is decoded from its base64 and bounded by the gateway, then goes through the **same Media Pipeline a workspace read uses** (docs/25): scanned, budgeted, its metadata stripped, its original and egress copies stored by digest. What a vision-capable model receives is the egress copy — the image without the metadata a server could hide an instruction in — in the split follow-up representation of REQ-EV-0188, named by the call it came from, while the canonical log keeps only digests. A block the pipeline refuses (over budget, not the type it claims) is reported as refused and no envelope is made for it. Proof: `qual_ev_0187_an_mcp_image_result_reaches_a_vision_capable_model_through_the_media_pipeline` (docs/58 MEDIA-E2E-006).

**A credential goes to a server to be used, not repeated.** Every answer is checked for the secrets in the Core's custody; one that comes back is replaced before the result leaves the host, the count is on the result, and the replacement is a `SecurityEventRecorded` (`SECRET_IN_EXTERNAL_RESULT`). Tool declarations the host refused are recorded the same way (`EXTERNAL_DECLARATION_REFUSED`) — what a server tried is evidence on the task, not a log line.

Proof: `qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport` — the real Core against real MCP server processes (`tools/mcp-testserver`, a conformant server over stdio), plus the `crates/mcp` unit suites for the bounds, the fingerprint and the reconciliation rules.

## Large results and OutputRef

Any result beyond inline budget is persisted as immutable OutputRef with mime/type, byte length, checksum, preview and paginated/ranged reads. Model gets concise metadata + selected slices. This prevents terminal/search/browser output from exploding context.

## Tool completion proof

Each tool has three test levels: schema/contract, real integration against its actual substrate, and Agent Runtime E2E. Tool code that exists but is not reachable through the Registry + policy + event loop is **not implemented** for completion accounting.


## V2 canonical inventory and source parity

`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` is normative for tool ownership. External-reference capability names are compatibility/provenance only. Every executable capability still passes ToolCallNormalizer → Capability Kernel → effector → typed result → Evidence/Effect Ledger. Rich tool media is normalized through Media Pipeline, and discovery/skill activation can never authorize execution.
