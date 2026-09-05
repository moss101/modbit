# Evidence-Derived Implementation Tasks

> This file prevents evidence-derived behaviors from being collapsed into vague milestone prose. Tasks are **behavioral obligations**, not invitations to create one file per row. Multiple tasks mapped to the same canonical owner should normally be implemented together when they share a vertical slice.

For every task, the agent must first audit existing code and classify the current state. `ALREADY EXISTS` is not a valid status without real qualification evidence.


## Agent Admission

### IMP-EV-0219 — No nested agent for certain profiles

- **Requirement:** `REQ-EV-0219`
- **Disposition:** ADOPT
- **Mandatory behavior:** Delegation depth/profile ceiling is explicit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0219` — Leaf profile lacks spawn capability.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Agent Profile Registry

### IMP-EV-0115 — File-defined custom agents

- **Requirement:** `REQ-EV-0115`
- **Disposition:** ADOPT
- **Mandatory behavior:** Custom agents are declarative profiles; Core compiles effective tools/capabilities.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0115` — Profile requesting forbidden tool receives narrowed surface.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0182 — Extension-provided subagents

- **Requirement:** `REQ-EV-0182`
- **Disposition:** ADAPT
- **Mandatory behavior:** Declarative agent profiles import into canonical Modbit profile schema.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0182` — Invalid/unsafe tool declaration is narrowed or rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0256 — Agent identity separate from persona/model

- **Requirement:** `REQ-EV-0256`
- **Disposition:** ADOPT
- **Mandatory behavior:** AgentNode identity persists while model/profile may change under policy.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0256` — Fallback model switch preserves agent/run identity and lineage.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Agent Runtime

### IMP-EV-0006 — Persisted child conversations

- **Requirement:** `REQ-EV-0006`
- **Disposition:** ADOPT
- **Mandatory behavior:** Child AgentNode state is durable with parent/root lineage and its own execution capsule.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0006` — Kill Core mid-child run; restart resumes exact child identity and parent linkage.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0007 — Idempotent subagent spawn

- **Requirement:** `REQ-EV-0007`
- **Disposition:** ADOPT
- **Mandatory behavior:** Spawn carries idempotency key; replay reattaches rather than duplicating work.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0007` — Replay identical spawn call after transport retry; exactly one child exists.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0008 — Foreground ↔ background transition

- **Requirement:** `REQ-EV-0008`
- **Disposition:** ADOPT
- **Mandatory behavior:** Preserve agent identity while changing scheduling/attention mode.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0008` — Move real running child background then foreground without restart or lost event offset.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0009 — Typed live steering

- **Requirement:** `REQ-EV-0009`
- **Disposition:** ADOPT
- **Mandatory behavior:** Steer/cancel/follow-up are typed control events, not chat conventions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0009` — Steer during live model/tool cycle and verify deterministic cancellation boundary and replay.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0046 — Detached-agent permission ceiling

- **Requirement:** `REQ-EV-0046`
- **Disposition:** ADOPT
- **Mandatory behavior:** Background agents can consume grants but cannot create interactive privilege expansion.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0046` — Background child reaches protected effect and transitions to parent attention state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0047 — Persisted AgentNode

- **Requirement:** `REQ-EV-0047`
- **Disposition:** ADOPT
- **Mandatory behavior:** Agent state and lineage are durable independent of process.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0047` — Core restart preserves child status, task, tool cursor and private context refs.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0048 — AgentExecutionCapsule

- **Requirement:** `REQ-EV-0048`
- **Disposition:** ADOPT
- **Mandatory behavior:** Per-agent context/tools/model policy/budgets/capability ceiling are explicit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0048` — Child cannot access a parent-only secret/tool or hidden transcript.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0049 — Agent parking

- **Requirement:** `REQ-EV-0049`
- **Disposition:** ADOPT
- **Mandatory behavior:** Park is durable state distinct from cancel/complete and preserves resumable context.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0049` — Park child during parent intervention, restart Core, resume from same state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0050 — Resume terminal child states

- **Requirement:** `REQ-EV-0050`
- **Disposition:** ADAPT
- **Mandatory behavior:** Policy may continue failed/cancelled/completed child with new prompt while preserving lineage.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0050` — Resume completed research child; new result is a new attempt with prior evidence linked.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0051 — Bounded recursive delegation

- **Requirement:** `REQ-EV-0051`
- **Disposition:** ADOPT
- **Mandatory behavior:** Nested delegation disabled by default; explicit max depth and capacity enforced transactionally.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0051` — Depth N+1 launch rejected with typed admission failure.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0053 — Stall/no-progress detection

- **Requirement:** `REQ-EV-0053`
- **Disposition:** ADAPT
- **Mandatory behavior:** Detect repeated low-novelty cycles and surface blocker rather than loop indefinitely.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0053` — Seed repeated read/search loop; watchdog moves run to STALLED with evidence.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0078 — Child-agent isolation

- **Requirement:** `REQ-EV-0078`
- **Disposition:** ADOPT
- **Mandatory behavior:** Children receive bounded capsule, not unrestricted parent transcript/authority.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0078` — Child prompt dump lacks parent-only memory and secrets.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0099 — Command failure ≠ turn failure

- **Requirement:** `REQ-EV-0099`
- **Disposition:** ADOPT
- **Mandatory behavior:** Nonzero exit is typed result and may feed repair loop.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0099` — Compile failure followed by fix/test succeeds in same run without task crash.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0107 — Failure output used for repair

- **Requirement:** `REQ-EV-0107`
- **Disposition:** ADOPT
- **Mandatory behavior:** Bounded failure evidence becomes next-round context with provenance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0107` — Seed failing test; model receives bounded failure and repairs without losing raw log.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0117 — Plan mode

- **Requirement:** `REQ-EV-0117`
- **Disposition:** ADOPT
- **Mandatory behavior:** Plan profile is mutation-disabled and may require review by risk/policy.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0117` — Attempt write in Plan mode is absent/denied before execution.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0127 — Autonomous detach/resume

- **Requirement:** `REQ-EV-0127`
- **Disposition:** ADOPT
- **Mandatory behavior:** Detached runs use leases/checkpoints/heartbeats/replay and permission ceiling.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0127` — Kill client; worker continues safely and reconnects.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0178 — Specialized subagents with tool/model profiles

- **Requirement:** `REQ-EV-0178`
- **Disposition:** ADAPT
- **Mandatory behavior:** Agent profile selects bounded tools/model policy/domain context inside AgentExecutionCapsule.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0178` — Launch two real specialized children and verify independent state/tool ceilings.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0179 — Background subagents with follow-up continuation

- **Requirement:** `REQ-EV-0179`
- **Disposition:** ADOPT
- **Mandatory behavior:** Background child remains addressable/resumable; follow-up is typed and prior output treated as evidence.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0179` — Complete child, restart parent, send follow-up and verify lineage/state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0218 — Isolated coder/explore/plan subagents

- **Requirement:** `REQ-EV-0218`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use profiles with bounded tools/context and explicit result envelopes.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0218` — Explore child cannot mutate; coder child mutation requires worktree/capability.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0220 — Resumable per-agent state

- **Requirement:** `REQ-EV-0220`
- **Disposition:** ADOPT
- **Mandatory behavior:** Persist child state independent of parent transcript.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0220` — Restart and resume child with same lineage.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0238 — Process-local background subagent durability limitation

- **Requirement:** `REQ-EV-0238`
- **Disposition:** ADAPT
- **Mandatory behavior:** Modbit improves by persisting AgentNode/protocol state so background child survives Core lifecycle.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0238` — Kill/restart test proves durability beyond process-local baseline.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0255 — One primary responsible agent

- **Requirement:** `REQ-EV-0255`
- **Disposition:** ADOPT
- **Mandatory behavior:** Default one primary agent owns task outcome; delegate only separable bounded work.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0255` — Typical task creates one primary; unnecessary swarm is not spawned.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0267 — Transactional subagent admission

- **Requirement:** `REQ-EV-0267`
- **Disposition:** ADOPT
- **Mandatory behavior:** Capacity, worktree/write-set, capability and child record admission commit atomically before child starts.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0267` — Injected failure during admission leaves no orphan child/worktree/capacity leak.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Agent/Task Runtime

### IMP-EV-0263 — Background tasks

- **Requirement:** `REQ-EV-0263`
- **Disposition:** ADOPT
- **Mandatory behavior:** Detached work uses durable task handles, permission ceiling, checkpoint/replay.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0263` — Client disconnect/restart does not lose background task.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Agent/Tool Profiles

### IMP-EV-0241 — Profiles/bundles

- **Requirement:** `REQ-EV-0241`
- **Disposition:** ADAPT
- **Mandatory behavior:** Profiles are declarative compiled config, not arbitrary runtime replacement.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0241` — Profile validation rejects unknown/unsafe capability expansion.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Approval Service

### IMP-EV-0194 — Multi-client permission mediation

- **Requirement:** `REQ-EV-0194`
- **Disposition:** ADAPT
- **Mandatory behavior:** One canonical approval owns state; clients may be designated/consensus by policy but model never resolves votes.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0194` — Conflicting client approvals follow configured policy and are auditable.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Approval/Question Service

### IMP-EV-0222 — Structured AskUserQuestion

- **Requirement:** `REQ-EV-0222`
- **Disposition:** ADAPT
- **Mandatory behavior:** Typed questions/options for disambiguation when interactive surface exists; text fallback is explicit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0222` — Headless run returns NEEDS_INPUT rather than hanging.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Architecture Governance

### IMP-EV-0208 — No second general memory system

- **Requirement:** `REQ-EV-0208`
- **Disposition:** ADOPT
- **Mandatory behavior:** Evolution wiki is scoped to Skill Lab and cannot become parallel Engineering Memory/recovery system.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0208` — Architecture dependency test shows production runtime has one Engineering Memory interface.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Artifact Store

### IMP-EV-0011 — Blob-addressed large run state

- **Requirement:** `REQ-EV-0011`
- **Disposition:** ADAPT
- **Mandatory behavior:** Large payloads are content-addressed and referenced from events/checkpoints.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0011` — Restart and retrieve multi-MB artifact by digest; digest mismatch fails closed.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0269 — OutputRef tool pagination

- **Requirement:** `REQ-EV-0269`
- **Disposition:** ADOPT
- **Mandatory behavior:** Large tool outputs return bounded preview plus durable range-addressable OutputRef.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0269` — 10MB+ tool result is paged without context overflow and digest matches raw output.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Artifact/Notebook Adapter

### IMP-EV-0186 — Structured notebook read/edit

- **Requirement:** `REQ-EV-0186`
- **Disposition:** ADAPT
- **Mandatory behavior:** Represent notebook cells structurally; edits target stable cell IDs and reject ambiguous/truncated state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0186` — Real ipynb read/edit preserves unrelated cells and execution metadata policy.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Attention Manager

### IMP-EV-0151 — Needs Attention UX

- **Requirement:** `REQ-EV-0151`
- **Disposition:** ADOPT
- **Mandatory behavior:** Aggregate approvals, conflicts, failures, stalls and questions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0151` — Each attention reason is actionable and clears from canonical event.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0275 — Reminder/decision engine

- **Requirement:** `REQ-EV-0275`
- **Disposition:** ADAPT
- **Mandatory behavior:** Host derives actionable reminders from unresolved approvals/questions/blockers/deadlines; no second scheduler.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0275` — Attention item is created/cleared solely from canonical unresolved state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Benchmark Harness

### IMP-EV-0250 — Measure input-token reduction

- **Requirement:** `REQ-EV-0250`
- **Disposition:** ADOPT
- **Mandatory behavior:** Track prompt input tokens for baseline vs treatment.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0250` — Benchmark report includes paired distribution/confidence.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0251 — Measure tool-call reduction

- **Requirement:** `REQ-EV-0251`
- **Disposition:** ADOPT
- **Mandatory behavior:** Count normalized tool calls per task.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0251` — Same model/task/environment across variants.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0252 — Measure agent-time reduction

- **Requirement:** `REQ-EV-0252`
- **Disposition:** ADOPT
- **Mandatory behavior:** Measure execution time separately from index build and also report cold-start total.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0252` — Both warm agent time and cold time-to-first-use reported.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Benchmark Method

### IMP-EV-0253 — Do not force retrieval tool usage in treatment

- **Requirement:** `REQ-EV-0253`
- **Disposition:** ADOPT
- **Mandatory behavior:** Agent chooses tools naturally so treatment is not biased.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0253` — Benchmark prompts are identical except available capability profile.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Browser Event Protocol

### IMP-EV-0279 — Incremental page-state deltas

- **Requirement:** `REQ-EV-0279`
- **Disposition:** ADOPT
- **Mandatory behavior:** After baseline snapshot, send bounded semantic deltas when safe; full rehydrate fallback.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0279` — Long navigation flow shows token reduction and state equivalence.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Browser Runtime

### IMP-EV-0110 — Browser/computer behind authenticated peer boundary

- **Requirement:** `REQ-EV-0110`
- **Disposition:** ADOPT
- **Mandatory behavior:** Browser worker attaches via authenticated session and cannot bypass Core policy.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0110` — Forged peer cannot attach/take over session.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0234 — Browser snapshot + vision tools

- **Requirement:** `REQ-EV-0234`
- **Disposition:** ADAPT
- **Mandatory behavior:** Structural semantic browser remains primary; targeted vision only when required.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0234` — Accessible and canvas fixtures validate escalation hierarchy.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0276 — Chromium as execution engine, not a new browser engine

- **Requirement:** `REQ-EV-0276`
- **Disposition:** ADOPT
- **Mandatory behavior:** Build agent-native semantic runtime on Chromium/CDP rather than replacing web engine.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0276` — Modern SaaS fixture executes with standard Chromium compatibility.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0280 — Page/action/state graph

- **Requirement:** `REQ-EV-0280`
- **Disposition:** ADAPT
- **Mandatory behavior:** Record observed transitions/compound actions as evidence/cache, not universal authority.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0280` — Known flow can reuse verified transition; changed page invalidates fingerprint.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0282 — Targeted visual escalation

- **Requirement:** `REQ-EV-0282`
- **Disposition:** ADOPT
- **Mandatory behavior:** Capture only visual regions that structural state cannot represent.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0282` — Canvas/image button fixture escalates locally; standard form does not.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Browser/Computer Runtime

### IMP-EV-0082 — Deterministic→accessibility→visual ladder

- **Requirement:** `REQ-EV-0082`
- **Disposition:** ADOPT
- **Mandatory behavior:** Use deterministic integration, semantic/AX state, then targeted pixels/raw input fallback.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0082` — Accessible form completes with zero screenshot dependency; canvas case escalates explicitly.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## CLI/API Surface

### IMP-EV-0126 — Headless mode

- **Requirement:** `REQ-EV-0126`
- **Disposition:** ADOPT
- **Mandatory behavior:** Same Core/Policy/Evidence contracts without UI-only tools.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0126` — Identical task run via desktop and headless yields same canonical states.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Capability Kernel

### IMP-EV-0043 — ProtocolCapabilitySet

- **Requirement:** `REQ-EV-0043`
- **Disposition:** ADOPT
- **Mandatory behavior:** Separate client/transport capabilities from per-round execution authority.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0043` — Headless client lacks UI-only capabilities while Core task remains valid.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0044 — End-to-end capability invariant

- **Requirement:** `REQ-EV-0044`
- **Disposition:** ADOPT
- **Mandatory behavior:** Advertise capability only when producer, authorization, transport and consumer all exist.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0044` — Remove browser consumer; browser tool disappears rather than failing after model selects it.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0080 — Policy before execution

- **Requirement:** `REQ-EV-0080`
- **Disposition:** ADOPT
- **Mandatory behavior:** Host authorization is security boundary, not model compliance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0080` — Prompt injection asking to bypass policy fails.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0133 — Tool visibility conditional on host support

- **Requirement:** `REQ-EV-0133`
- **Disposition:** ADOPT
- **Mandatory behavior:** End-to-end capability invariant prevents dead tools.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0133` — Disable consumer adapter and verify schema disappears.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Change Engine

### IMP-EV-0014 — Canonical edit transaction

- **Requirement:** `REQ-EV-0014`
- **Disposition:** ADOPT
- **Mandatory behavior:** Normalize → precondition/hash → stage → authorize → validate → journal → atomic apply.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0014` — Concurrent user edit causes precondition failure without data loss.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0015 — Deterministic edit match ladder

- **Requirement:** `REQ-EV-0015`
- **Disposition:** ADOPT
- **Mandatory behavior:** Exact → safe whitespace remap → contextual suggestion → ambiguity error; never guess.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0015` — Ambiguous duplicated target must fail and leave worktree unchanged.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0016 — Sequential multi-edit transaction

- **Requirement:** `REQ-EV-0016`
- **Disposition:** ADAPT
- **Mandatory behavior:** Apply ordered edits with per-step validation and rollback semantics.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0016` — Injected failure at edit N rolls back transaction or emits explicit partial state by contract.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0064 — Typed UndoPlan

- **Requirement:** `REQ-EV-0064`
- **Disposition:** ADOPT
- **Mandatory behavior:** Undo uses typed inverse actions rather than blind checkout.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0064` — Undo created/deleted/modified files while preserving unrelated user changes.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0065 — Optimistic-concurrency revert

- **Requirement:** `REQ-EV-0065`
- **Disposition:** ADOPT
- **Mandatory behavior:** Revert checks expected post-edit hash/revision before applying inverse.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0065` — User edit after agent change blocks destructive revert.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0067 — MergeTransaction

- **Requirement:** `REQ-EV-0067`
- **Disposition:** ADOPT
- **Mandatory behavior:** Persist source/target/base, conflicts/resolutions, validation and commit/rollback state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0067` — Injected conflict + failed test leaves merge transaction inspectable and recoverable.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0106 — Typed file-change patch/diff events

- **Requirement:** `REQ-EV-0106`
- **Disposition:** ADOPT
- **Mandatory behavior:** Every write produces structured revision-bound change event.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0106` — Apply real patch and verify UI/evidence sees identical diff.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Checkpoint Store

### IMP-EV-0012 — Checkpoint epoch fencing

- **Requirement:** `REQ-EV-0012`
- **Disposition:** ADOPT
- **Mandatory behavior:** Monotonic checkpoint epoch rejects stale asynchronous writers.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0012` — Race two checkpoint writes; old epoch cannot overwrite newer state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0013 — Checkpoint delta journal

- **Requirement:** `REQ-EV-0013`
- **Disposition:** ADOPT
- **Mandatory behavior:** Checkpoint records runtime/evidence cursor plus worktree deltas instead of transcript snapshots.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0013` — Restore edited worktree and protocol cursor after process restart from baseline+delta.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Computer Runtime

### IMP-EV-0083 — Exact application identity

- **Requirement:** `REQ-EV-0083`
- **Disposition:** ADOPT
- **Mandatory behavior:** Resolve approved app/window/process identity before actions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0083` — Window title spoof cannot substitute a different process identity.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0084 — Single controller lock

- **Requirement:** `REQ-EV-0084`
- **Disposition:** ADOPT
- **Mandatory behavior:** Only one automation controller owns a target/session at a time.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0084` — Two workers contend; second receives lease conflict.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0085 — Watchdog / emergency stop

- **Requirement:** `REQ-EV-0085`
- **Disposition:** ADOPT
- **Mandatory behavior:** Host-owned timeout/kill independent of model loop.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0085` — Emergency stop halts input within safety bound and records reason.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0087 — Human activity preemption

- **Requirement:** `REQ-EV-0087`
- **Disposition:** ADOPT
- **Mandatory behavior:** Recent human activity revokes/parks controller and enforces cooldown.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0087` — Inject real mouse/keyboard event; automation stops and requires reacquisition.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0089 — Typed computer-use failure taxonomy

- **Requirement:** `REQ-EV-0089`
- **Disposition:** ADOPT
- **Mandatory behavior:** Emit stable failures: TARGET_STALE, TARGET_OCCLUDED, WINDOW_UNVERIFIABLE, ACCESSIBILITY_UNAVAILABLE, HUMAN_ACTIVE, MODAL_BLOCKING, TARGET_NOT_EDITABLE, ACTION_UNSAFE, PERMISSION_REQUIRED.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0089` — Fault fixtures trigger each code and verify recovery guidance.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0090 — Safe typing + clipboard guard

- **Requirement:** `REQ-EV-0090`
- **Disposition:** ADOPT
- **Mandatory behavior:** Prefer reversible entry; preserve/restore clipboard and verify destination before replacement.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0090` — Clipboard secret is restored and never enters model/evidence body.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Computer Runtime + Evidence

### IMP-EV-0086 — Verified fallback + evidence

- **Requirement:** `REQ-EV-0086`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fallback action records modality/reason/target and verifies post-state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0086` — Raw input fallback without post-check is rejected by completion verifier.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Configuration Service

### IMP-EV-0039 — Typed ConfigurationResolver

- **Requirement:** `REQ-EV-0039`
- **Disposition:** ADOPT
- **Mandatory behavior:** Use domain-specific merge laws for permissions, MCP, hooks, rules, network and model policy with provenance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0039` — Conflicting admin/project/user configs resolve deterministically; lower authority cannot widen.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context + Change Engine

### IMP-EV-0168 — Retrieve before edit

- **Requirement:** `REQ-EV-0168`
- **Disposition:** ADOPT
- **Mandatory behavior:** Material mutation requires adequate repository understanding.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0168` — Blind edit attempt with missing context is blocked/surfaced.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context + Session Stores

### IMP-EV-0092 — Transcript compaction with recoverability

- **Requirement:** `REQ-EV-0092`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compact active context while retaining lossless canonical events/state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0092` — Restart after multiple compactions and reconstruct exact task/protocol state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Connectors

### IMP-EV-0161 — Connected engineering context

- **Requirement:** `REQ-EV-0161`
- **Disposition:** ADOPT
- **Mandatory behavior:** Approved specs/issues/design docs enter provenance-labeled context.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0161` — Prompt injection in ticket remains untrusted data and cannot grant tools.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Economy

### IMP-EV-0111 — Prompt cache key + compaction API awareness

- **Requirement:** `REQ-EV-0111`
- **Disposition:** ADAPT
- **Mandatory behavior:** Track cacheable stable prefix/key economics without copying proprietary algorithm.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0111` — Benchmark reports cached-prefix hit/miss and compaction invalidation correctness.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0173 — Context efficiency metrics

- **Requirement:** `REQ-EV-0173`
- **Disposition:** ADOPT
- **Mandatory behavior:** Measure verified outcome per token/latency/cost.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0173` — Benchmark dashboard reports quality and economics together.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0268 — Prompt-cache-aware compaction

- **Requirement:** `REQ-EV-0268`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compaction and prompt assembly account for stable cacheable prefixes and cache invalidation.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0268` — Benchmark cache hits/misses and verify no stale context after fork/revert.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0274 — Measured context/tool-result savings

- **Requirement:** `REQ-EV-0274`
- **Disposition:** ADOPT
- **Mandatory behavior:** Measure tokens/tool calls/latency saved by compaction, OutputRef and retrieval choices against baseline.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0274` — Paired benchmark publishes verified-outcome economics.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Engine

### IMP-EV-0001 — Dynamic context query planning

- **Requirement:** `REQ-EV-0001`
- **Disposition:** ADAPT
- **Mandatory behavior:** Classify query intent and escalate L0 exact → L1 hybrid → L2 structural → L3 engineering only when required.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0001` — Fixed-revision repository benchmark proves planner choice, recall, latency and token cost.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0002 — Read-through freshness hydration

- **Requirement:** `REQ-EV-0002`
- **Disposition:** ADOPT
- **Mandatory behavior:** Indexes rank candidates, but source bytes are re-read from the active revision before prompt inclusion.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0002` — Mutate indexed file after indexing; stale bytes must never reach model context.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0003 — Signature-only retrieval stubs

- **Requirement:** `REQ-EV-0003`
- **Disposition:** ADAPT
- **Mandatory behavior:** Lower-ranked candidates use symbol/signature stubs with lazy hydration.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0003` — Context budget test proves hydration occurs only when requested and provenance survives.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0056 — ContextEpoch

- **Requirement:** `REQ-EV-0056`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compaction creates versioned model-visible epoch without changing run truth.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0056` — Fork/revert invalidates incompatible compaction output and retains canonical history.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0057 — CompactionManifest + hot/warm/cold context

- **Requirement:** `REQ-EV-0057`
- **Disposition:** ADOPT
- **Mandatory behavior:** Track source head, preserved facts/IDs, resources and compressed projection.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0057` — Compaction fidelity test checks labeled instructions/decisions/approvals survive.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0058 — Async compaction stale-result guard

- **Requirement:** `REQ-EV-0058`
- **Disposition:** ADOPT
- **Mandatory behavior:** Reject async compaction if source epoch/head advanced; synchronous bounded fallback under pressure.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0058` — Delay compactor while adding events; stale result cannot install.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0130 — Conversation compaction

- **Requirement:** `REQ-EV-0130`
- **Disposition:** ADOPT
- **Mandatory behavior:** ContextEpoch + manifest over lossless durable history.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0130` — Critical-fact compaction corpus meets fidelity threshold.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0153 — Semantic code retrieval

- **Requirement:** `REQ-EV-0153`
- **Disposition:** ADOPT
- **Mandatory behavior:** Meaning-based retrieval returns revision/provenance candidates.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0153` — Repo-QA benchmark measures recall@K and precision.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0154 — Lexical + semantic fusion

- **Requirement:** `REQ-EV-0154`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fuse exact/BM25/semantic with task-aware rerank.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0154` — A/B benchmark vs lexical and semantic-only baselines.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0158 — Commit history

- **Requirement:** `REQ-EV-0158`
- **Disposition:** ADOPT
- **Mandatory behavior:** Use Git history/blame as subordinate context with current-source priority.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0158` — Old commit cannot override current code truth.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0159 — Freshness/relevance

- **Requirement:** `REQ-EV-0159`
- **Disposition:** ADOPT
- **Mandatory behavior:** Rank active/current knowledge and attach revision validity.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0159` — Deprecated doc is downranked after source change.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0165 — Reranking

- **Requirement:** `REQ-EV-0165`
- **Disposition:** ADOPT
- **Mandatory behavior:** Rank by task intent/revision/proximity/coverage/provenance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0165` — Rerank improves relevant-file recall without unacceptable latency.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0167 — Context compression

- **Requirement:** `REQ-EV-0167`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compress lower-priority data with recoverable handles/provenance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0167` — Compression fidelity corpus and handle hydration pass.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0169 — Context provenance

- **Requirement:** `REQ-EV-0169`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fragments carry source/repo/revision/hash/retrieval reason.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0169` — Prompt envelope validates provenance on every non-ephemeral fragment.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0170 — Temporal validity

- **Requirement:** `REQ-EV-0170`
- **Disposition:** ADOPT
- **Mandatory behavior:** Evaluate staleness/current worktree/source revision.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0170` — Stale cache never labeled current.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0171 — Context Engine as shared service/ports

- **Requirement:** `REQ-EV-0171`
- **Disposition:** ADOPT
- **Mandatory behavior:** All agents/reviewers/browser/testing use same context ports.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0171` — Architecture test prevents duplicate search stacks in production modules.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0174 — Fast Context subagent

- **Requirement:** `REQ-EV-0174`
- **Disposition:** ADAPT
- **Mandatory behavior:** Bounded read-only retrieval specialist can build ContextPack.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0174` — Specialist has no mutation tools and produces provenance-complete pack.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0249 — Hybrid exact/BM25/vector retrieval baseline

- **Requirement:** `REQ-EV-0249`
- **Disposition:** ADOPT
- **Mandatory behavior:** Use as external baseline class, enhanced with structural signals.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0249` — Run same frozen retrieval benchmark profile with and without Modbit retrieval.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Graph

### IMP-EV-0005 — Shared AST/symbol chunk representation

- **Requirement:** `REQ-EV-0005`
- **Disposition:** ADOPT
- **Mandatory behavior:** One structural representation feeds retrieval, prompt packing and impact analysis.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0005` — Cross-language fixture proves symbol identity is consistent across index/query/impact paths.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0155 — Code structure mapping

- **Requirement:** `REQ-EV-0155`
- **Disposition:** ADOPT
- **Mandatory behavior:** Represent files/modules/symbols/contracts/dependencies.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0155` — Cross-file query resolves structural path correctly.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0157 — Dependency/call graph

- **Requirement:** `REQ-EV-0157`
- **Disposition:** ADOPT
- **Mandatory behavior:** Traverse callers/callees/contracts/ownership/impact.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0157` — Impact benchmark checks affected file/test recall.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0164 — Graph expansion

- **Requirement:** `REQ-EV-0164`
- **Disposition:** ADOPT
- **Mandatory behavior:** Bounded expansion from symbol to callers/tests/config/evidence.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0164` — Budget cap prevents runaway expansion.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Inspector

### IMP-EV-0131 — Context window breakdown

- **Requirement:** `REQ-EV-0131`
- **Disposition:** ADOPT
- **Mandatory behavior:** Expose composition, token cost, source, freshness and reasons.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0131` — Inspector totals match actual provider request envelope.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Pack Compiler

### IMP-EV-0166 — Token-budget packing

- **Requirement:** `REQ-EV-0166`
- **Disposition:** ADOPT
- **Mandatory behavior:** Pack highest value fragments under explicit budget/coverage priorities.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0166` — Budget never exceeded and required critical facts retained.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Policy

### IMP-EV-0203 — Inference agent isolated from evolution wiki

- **Requirement:** `REQ-EV-0203`
- **Disposition:** ADOPT
- **Mandatory behavior:** Production agent receives approved skill, not raw optimization wiki/traces unless task separately retrieves authorized evidence.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0203` — Prompt audit confirms evolution store is absent during normal run.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context Query Planner

### IMP-EV-0163 — Query decomposition

- **Requirement:** `REQ-EV-0163`
- **Disposition:** ADOPT
- **Mandatory behavior:** Break broad requests into targeted retrieval operations.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0163` — Planner benchmark records subqueries and coverage.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Context/Policy

### IMP-EV-0284 — Prompt-injection isolation in web content

- **Requirement:** `REQ-EV-0284`
- **Disposition:** ADOPT
- **Mandatory behavior:** Web page text is untrusted data; cannot alter policy/tool authority.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0284` — Seed hostile page instructions; forbidden tool remains unavailable.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Coordinator + TaskContract

### IMP-EV-0144 — Captain→Build separation

- **Requirement:** `REQ-EV-0144`
- **Disposition:** ADAPT
- **Mandatory behavior:** Coordinator delegates typed bounded TaskContracts; builder cannot widen scope.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0144` — Builder attempts out-of-scope file write and is denied.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Core + Worker Fabric

### IMP-EV-0076 — RPC/session-state separation

- **Requirement:** `REQ-EV-0076`
- **Disposition:** ADOPT
- **Mandatory behavior:** UI/worker messages are typed; Core remains canonical session owner.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0076` — Kill renderer and reconnect; session truth unchanged.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Core API

### IMP-EV-0192 — Daemon multi-client HTTP+SSE event model

- **Requirement:** `REQ-EV-0192`
- **Disposition:** ADAPT
- **Mandatory behavior:** Multiple clients attach to one session via replayable event stream without sharing UI-local truth.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0192` — Desktop + web test observe same run; reconnect from event cursor is lossless.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Core Architecture

### IMP-EV-0242 — Durable facts vs live interception separation

- **Requirement:** `REQ-EV-0242`
- **Disposition:** ADOPT
- **Mandatory behavior:** Durable state remains in stores; live hooks are ephemeral control.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0242` — Restart loses no durable truth even though hook process resets.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Desktop Security

### IMP-EV-0103 — Renderer→bridge→privileged host boundaries

- **Requirement:** `REQ-EV-0103`
- **Disposition:** ADOPT
- **Mandatory behavior:** Renderer has narrow authenticated IPC to Core/native services.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0103` — Malicious renderer message without schema/capability rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Diagnostics Adapter

### IMP-EV-0020 — Pull-based diagnostics

- **Requirement:** `REQ-EV-0020`
- **Disposition:** ADOPT
- **Mandatory behavior:** Diagnostics are fetched after settle/on demand, not injected continuously.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0020` — High-churn editor/test fixture proves no unsolicited diagnostic prompt traffic.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Domain Model

### IMP-EV-0102 — Thread state distinct from turn state

- **Requirement:** `REQ-EV-0102`
- **Disposition:** ADOPT
- **Mandatory behavior:** Session/task/turn/step/tool states have separate state machines.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0102` — State-transition tests reject impossible conflation such as command failure=thread failure.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Effect Ledger

### IMP-EV-0066 — Reversibility/compensation classes

- **Requirement:** `REQ-EV-0066`
- **Disposition:** ADOPT
- **Mandatory behavior:** Classify reversible/partially reversible/compensatable/irreversible effects.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0066` — External API effect is never labeled fully undoable; compensation receipt is distinct.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0270 — Protected-effect receipt chain

- **Requirement:** `REQ-EV-0270`
- **Disposition:** ADOPT
- **Mandatory behavior:** High-risk effects produce immutable hash-linked receipt chain bound to approval/capability/call/result.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0270` — Tamper/delete/reorder receipt causes chain verification failure.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Engineering Memory

### IMP-EV-0162 — Organizational engineering memory

- **Requirement:** `REQ-EV-0162`
- **Disposition:** ADOPT
- **Mandatory behavior:** Validated facts with scope/provenance/TTL/edit/delete.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0162` — Memory conflict/supersession is inspectable and no raw transcript auto-promotes.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Error Service

### IMP-EV-0017 — Dual user/model error channels

- **Requirement:** `REQ-EV-0017`
- **Disposition:** ADAPT
- **Mandatory behavior:** One canonical error identity renders safe user explanation and structured model repair payload.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0017` — Secret-bearing internal error is redacted for both surfaces according to policy.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Event Protocol

### IMP-EV-0010 — Offset-key event resume

- **Requirement:** `REQ-EV-0010`
- **Disposition:** ADOPT
- **Mandatory behavior:** Every run event has monotonic offset; clients reconnect from last offset with full-rehydrate fallback.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0010` — Disconnect desktop, produce events, reconnect from offset and compare exact stream.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0098 — Typed tool result re-entry

- **Requirement:** `REQ-EV-0098`
- **Disposition:** ADOPT
- **Mandatory behavior:** Tool results return as typed RunStep/ModelEvent payloads.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0098` — Provider replay preserves tool-call/result pairing across restart.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Evidence Archive

### IMP-EV-0147 — Browser/E2E evidence

- **Requirement:** `REQ-EV-0147`
- **Disposition:** ADOPT
- **Mandatory behavior:** Normalize browser/runtime/test evidence to exact revision/run step.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0147` — Visual/browser test artifact links to revision and verification claim.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Evidence Index

### IMP-EV-0132 — Searchable transcript/tool evidence

- **Requirement:** `REQ-EV-0132`
- **Disposition:** ADOPT
- **Mandatory behavior:** Index messages/tools/commands/files/errors/checkpoints metadata.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0132` — Search returns evidence by run/step and respects tenant scope.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Execution Timeline

### IMP-EV-0077 — Session branching

- **Requirement:** `REQ-EV-0077`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fork from RunStep/Checkpoint with worktree/evidence/context carryover.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0077` — Fork produces independent revision lineage without copying invalid pending effects.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0122 — Fork session

- **Requirement:** `REQ-EV-0122`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fork from step/checkpoint with BranchCarryoverCapsule.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0122` — New branch gets selected decisions/evidence but no stale pending approval.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0123 — Rewind/session tree

- **Requirement:** `REQ-EV-0123`
- **Disposition:** ADOPT
- **Mandatory behavior:** Preview/revert/fork operations over run DAG are explicit and auditable.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0123` — Preview is non-mutating; revert honors optimistic hash checks.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## ExecutionBackend

### IMP-EV-0109 — Separate local and cloud execution contracts

- **Requirement:** `REQ-EV-0109`
- **Disposition:** ADOPT
- **Mandatory behavior:** One canonical execution interface has local/cloud adapters.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0109` — Same fixture passes on local and cloud with equivalent effect/event semantics.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0233 — Broad terminal backends

- **Requirement:** `REQ-EV-0233`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use replaceable local/cloud/private backends under one contract; do not adopt consumer/serverless vendors by default.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0233` — Backend conformance suite runs same fixture on supported backends.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0291 — Replaceable sandbox backend boundary

- **Requirement:** `REQ-EV-0291`
- **Disposition:** ADOPT
- **Mandatory behavior:** MicroVM substrate implementation details cannot leak into agent/tool/domain contracts.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0291` — Backend contract test can run against local reference backend and MicroVM substrate backend.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Extension System

### IMP-EV-0138 — Unified plugins with commands/tools/hooks/providers

- **Requirement:** `REQ-EV-0138`
- **Disposition:** ADAPT
- **Mandatory behavior:** One governed package surface outside trusted Core.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0138` — Extension crash/timeout cannot bypass Core or corrupt run state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0183 — Extension context/commands/MCP resources

- **Requirement:** `REQ-EV-0183`
- **Disposition:** ADAPT
- **Mandatory behavior:** Support compatible external instruction manifest/commands/skills/agents/MCP through importer, not an external-reference runtime.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0183` — Compatibility fixture imports and migration report labels mapped/skipped/conflicts.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0225 — Marketplace trust surfaced

- **Requirement:** `REQ-EV-0225`
- **Disposition:** ADOPT
- **Mandatory behavior:** Show publisher/source/signature/capabilities before activation.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0225` — Unsigned/untrusted extension is quarantined.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Guest RPC

### IMP-EV-0289 — Typed capability/effect-bound guest RPC

- **Requirement:** `REQ-EV-0289`
- **Disposition:** ADOPT
- **Mandatory behavior:** Guest operations use versioned typed RPC carrying task/effect/capability identity.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0289` — Unknown/stale RPC version/capability is rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Hook Bus

### IMP-EV-0042 — Typed lifecycle hooks

- **Requirement:** `REQ-EV-0042`
- **Disposition:** ADOPT
- **Mandatory behavior:** Typed before/after run/model/tool/change/verification/compaction hooks with timeout, scope and audit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0042` — Slow/failing hook follows configured fail policy and cannot bypass monotonic guard.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0139 — Lifecycle hooks

- **Requirement:** `REQ-EV-0139`
- **Disposition:** ADOPT
- **Mandatory behavior:** Typed governed interception/observation with timeout/fail policy.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0139` — Mutating hook cannot override final monotonic deny.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0240 — Composable lifecycle registrations

- **Requirement:** `REQ-EV-0240`
- **Disposition:** ADAPT
- **Mandatory behavior:** Plugins register typed reversible handlers outside Core.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0240` — Unload extension removes handlers without stale mutation path.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Importers

### IMP-EV-0137 — Import config from other agents

- **Requirement:** `REQ-EV-0137`
- **Disposition:** ADAPT
- **Mandatory behavior:** Import compatible skills/agents/rules/MCP with migration report and trust gate.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0137` — Malicious executable config is quarantined until user trusts.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Input Gateway

### IMP-EV-0190 — Channel image/file ingestion

- **Requirement:** `REQ-EV-0190`
- **Disposition:** ADAPT
- **Mandatory behavior:** Normalize attachments to MediaEnvelope with tenant/task/source provenance; no consumer chat-product scope required.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0190` — Upload image/file through desktop/API and verify same canonical envelope.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Input Queue

### IMP-EV-0191 — steer / collect / followup dispatch modes

- **Requirement:** `REQ-EV-0191`
- **Disposition:** ADOPT
- **Mandatory behavior:** Implement typed SteeringPolicy: interrupt-and-replace, coalesce after current, or ordered separate turns.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0191` — Concurrency test sends messages mid-run and verifies exact ordering/cancellation semantics.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0261 — Non-disruptive side question

- **Requirement:** `REQ-EV-0261`
- **Disposition:** ADAPT
- **Mandatory behavior:** Side question can use bounded recent/context snapshot without mutating main task state unless explicitly applied.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0261` — Ask side question mid-run; main state/event cursor remains unchanged.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0262 — Queued prompts

- **Requirement:** `REQ-EV-0262`
- **Disposition:** ADOPT
- **Mandatory behavior:** Queue/collect/follow-up semantics are explicit durable input events.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0262` — Multiple queued inputs preserve ordering across reconnect.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Instruction + Memory

### IMP-EV-0129 — Memory/project instruction layering

- **Requirement:** `REQ-EV-0129`
- **Disposition:** ADOPT
- **Mandatory behavior:** Scoped provenance-bound instructions/memory with precedence, TTL and conflict diagnostics.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0129` — Conflicting rules show explicit winner and source.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Instruction Compiler

### IMP-EV-0059 — Path-scoped lazy rules

- **Requirement:** `REQ-EV-0059`
- **Disposition:** ADOPT
- **Mandatory behavior:** Load rules when matching paths/symbols become active, with deterministic precedence.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0059` — Unrelated module rule stays absent until matching file is touched.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0105 — Skills + workspace rules injected explicitly

- **Requirement:** `REQ-EV-0105`
- **Disposition:** ADOPT
- **Mandatory behavior:** InstructionManifest records selected skill/rules source/version/reason.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0105` — Prompt trace proves rules/skills present only when selected and survive compaction.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## MCP Hub

### IMP-EV-0104 — MCP list / call / cancel lifecycle

- **Requirement:** `REQ-EV-0104`
- **Disposition:** ADOPT
- **Mandatory behavior:** External tools support discovery, call, cancellation with task/turn/call identity.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0104` — Real MCP test server supports list/call/cancel and audit correlation.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0128 — MCP management + scoped auth

- **Requirement:** `REQ-EV-0128`
- **Disposition:** ADOPT
- **Mandatory behavior:** Scopes, credential broker, health, lazy discovery and audit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0128` — User/project MCP conflict resolves deterministically; credentials never enter model prompt.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0193 — Workspace-scoped MCP transport pool

- **Requirement:** `REQ-EV-0193`
- **Disposition:** ADAPT
- **Mandatory behavior:** Share healthy MCP transports by normalized config fingerprint while isolating sessions/tenants.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0193` — Two sessions reuse transport; config/tenant change creates separate pool entry.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0224 — MCP config conversational management

- **Requirement:** `REQ-EV-0224`
- **Disposition:** ADAPT
- **Mandatory behavior:** UI/agent may propose config changes but host validates/trusts/authorizes.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0224` — Proposed MCP install cannot execute until trust/credential gates pass.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## MCP Hub + Media

### IMP-EV-0187 — Rich MCP media results

- **Requirement:** `REQ-EV-0187`
- **Disposition:** ADOPT
- **Mandatory behavior:** Normalize text/image/audio/file/resource outputs into typed ToolResult media parts.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0187` — MCP test server returns image+text; both reach vision-capable model and evidence store.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## MCP/Browser Gateway

### IMP-EV-0281 — Native browser-native structured-action research/site tools precedence

- **Requirement:** `REQ-EV-0281`
- **Disposition:** ADAPT
- **Mandatory behavior:** Prefer authenticated site-declared structured tool when available, then derived semantic action, primitive, vision.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0281` — Same task selects native tool when trust/policy allow; fallback works otherwise.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Media + File Tool

### IMP-EV-0184 — Multimodal read_file

- **Requirement:** `REQ-EV-0184`
- **Disposition:** ADOPT
- **Mandatory behavior:** Text and supported image/PDF/audio/video return typed media parts according to model capability.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0184` — Read actual PNG/PDF/audio/video with capable and incapable models; unsupported modality is explicit.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Media Pipeline

### IMP-EV-0185 — Bounded PDF text→vision fallback

- **Requirement:** `REQ-EV-0185`
- **Disposition:** ADOPT
- **Mandatory behavior:** Try text extraction first; bounded page render/transcription fallback is labeled lossy/untrusted.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0185` — Scanned PDF fixture triggers bounded vision path; page range/source/model recorded.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0223 — ReadMediaFile / media budgets

- **Requirement:** `REQ-EV-0223`
- **Disposition:** ADOPT
- **Mandatory behavior:** Media reads enforce edge/byte/page/duration budgets and targeted crop/full-resolution escalation.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0223` — Oversized image/video is bounded; explicit crop improves targeted recognition.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Model Gateway

### IMP-EV-0028 — Model capability catalog

- **Requirement:** `REQ-EV-0028`
- **Disposition:** ADOPT
- **Mandatory behavior:** Record context/output/tool/parallel/vision/reasoning/structured-output/cost/latency/health properties.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0028` — Provider conformance probes detect capability mismatch and route away.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0112 — Provider/model/effort/service-tier envelope

- **Requirement:** `REQ-EV-0112`
- **Disposition:** ADAPT
- **Mandatory behavior:** Expose provider-neutral requested/resolved model, reasoning effort and service tier metadata.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0112` — Routing record shows requested vs resolved values and policy reason.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0189 — Model vision/agent/modalities metadata

- **Requirement:** `REQ-EV-0189`
- **Disposition:** ADOPT
- **Mandatory behavior:** Capability catalog includes vision/agent and media modalities plus tool-result formatting support.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0189` — Routing refuses unsupported media model and selects eligible endpoint.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Model Router

### IMP-EV-0029 — Task fingerprint + capability-based routing

- **Requirement:** `REQ-EV-0029`
- **Disposition:** ADAPT
- **Mandatory behavior:** Apply hard policy/capability filters first, then eval-gated quality/cost/latency optimization.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0029` — Replay benchmark corpus and verify deterministic hard exclusions plus auditable scores.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0030 — Specialist model chains

- **Requirement:** `REQ-EV-0030`
- **Disposition:** ADAPT
- **Mandatory behavior:** Profiles may define bounded preferred/fallback models without hidden orchestration.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0030` — Primary outage triggers approved fallback and records RouterDecisionRecord.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Observability

### IMP-EV-0023 — Cloud SLO event ladder

- **Requirement:** `REQ-EV-0023`
- **Disposition:** ADOPT
- **Mandatory behavior:** Record request→prewarm→sandbox requested→ready→first token/tool stages.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0023` — Staging run emits all timestamps and derived cold/warm latency metrics.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Operations

### IMP-EV-0142 — Trace/status/export diagnostics

- **Requirement:** `REQ-EV-0142`
- **Disposition:** ADOPT
- **Mandatory behavior:** Provide doctor/trace/run export with secret redaction.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0142` — Export can replay evidence metadata and contains no credential values.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Parallel Change Coordinator

### IMP-EV-0150 — Large-scale parallel execution

- **Requirement:** `REQ-EV-0150`
- **Disposition:** ADAPT
- **Mandatory behavior:** Parallelism only for separable tasks with conflicts/merge verification.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0150` — Overlapping writes are serialized/denied before execution.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Persistence

### IMP-EV-0101 — Separate durable stores / restart-resume

- **Requirement:** `REQ-EV-0101`
- **Disposition:** ADOPT
- **Mandatory behavior:** Keep event/protocol/context/memory/artifact concerns separated and resumable.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0101` — Hard-kill backend and resume pending run without transcript inference.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Policy + Workspace Fabric

### IMP-EV-0093 — Workspace trust + sandbox separation

- **Requirement:** `REQ-EV-0093`
- **Disposition:** ADOPT
- **Mandatory behavior:** Workspace trust is distinct from fs/network/secret/tool sandbox grants.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0093` — Trusted repo still cannot use denied network/secret capability.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Policy Kernel

### IMP-EV-0031 — Enterprise model policy

- **Requirement:** `REQ-EV-0031`
- **Disposition:** ADOPT
- **Mandatory behavior:** Org policy can require/recommend/block model/provider/endpoints and cannot be weakened downstream.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0031` — Blocked provider remains unavailable despite task/profile request.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0040 — DevicePolicy / machine authority

- **Requirement:** `REQ-EV-0040`
- **Disposition:** ADAPT
- **Mandatory behavior:** Represent device/MDM trust roots, proxy, update, sandbox and telemetry constraints above project config.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0040` — Project file attempting to disable device requirement is rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0041 — Policy generation + hot revalidation

- **Requirement:** `REQ-EV-0041`
- **Disposition:** ADOPT
- **Mandatory behavior:** Refresh policy between model rounds; tightening can pause next round without mutating in-flight snapshot.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0041` — Change org policy while run active; current authorized tool finishes, next forbidden tool is absent.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0045 — Capability-oriented autonomous mode

- **Requirement:** `REQ-EV-0045`
- **Disposition:** ADOPT
- **Mandatory behavior:** Unattended execution is explicit bounded capability profile, never bypass/yolo.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0045` — Autonomous run cannot request higher privilege than profile ceiling.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0088 — Semantic UI risk classification

- **Requirement:** `REQ-EV-0088`
- **Disposition:** ADOPT
- **Mandatory behavior:** Risk = target × action × data × context, with credentials/security/unknown targets elevated.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0088` — Destructive/credential UI action requests approval even if click tool normally allowed.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0091 — Admin policy precedence

- **Requirement:** `REQ-EV-0091`
- **Disposition:** ADOPT
- **Mandatory behavior:** Device/org restrictions dominate user/project/task/model.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0091` — Project instruction cannot weaken org deny rule.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Policy Profiles

### IMP-EV-0136 — Permission modes

- **Requirement:** `REQ-EV-0136`
- **Disposition:** ADAPT
- **Mandatory behavior:** Friendly modes compile to monotonic policy; no model-controlled bypass.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0136` — Mode switch requiring user action cannot be triggered by model tool call.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Procedural Tool Runtime

### IMP-EV-0097 — Minimal code-mode interface: exec / wait / request_user_input

- **Requirement:** `REQ-EV-0097`
- **Disposition:** ADOPT
- **Mandatory behavior:** Expose minimal stable composition primitives while governed tools.* remain programmatically callable.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0097` — Real coding task completes through procedural mode and every nested effect is policy/evidence-tracked.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0231 — Tool RPC composition / execute code

- **Requirement:** `REQ-EV-0231`
- **Disposition:** ADAPT
- **Mandatory behavior:** Allow isolated code composition over governed tools.* with execution/time/output limits.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0231` — Script attempts unauthorized tool and is denied by same Kernel.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Protocol Store

### IMP-EV-0055 — Protocol-state persistence

- **Requirement:** `REQ-EV-0055`
- **Disposition:** ADOPT
- **Mandatory behavior:** Persist tool/approval/question/terminal/browser/subagent lifecycle needed for exact resume.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0055` — Crash while tool awaits approval; restart reconstructs exact pending state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Provider Adapter

### IMP-EV-0188 — Split tool media for provider compliance

- **Requirement:** `REQ-EV-0188`
- **Disposition:** ADOPT
- **Mandatory behavior:** Provider adapter can transform media placement without losing semantics; canonical ModelEvent stays provider-neutral.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0188` — Strict OpenAI-compatible test rejects embedded media but passes split follow-up representation.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Qualification Suite

### IMP-EV-0211 — Multi-level tests including real API call

- **Requirement:** `REQ-EV-0211`
- **Disposition:** ADOPT
- **Mandatory behavior:** Qualification includes direct component, registry/integration and real external API test when integration requires it.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0211` — Staging integration uses real test credential and recorded safe fixture; mock-only cannot pass.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Quality Gate

### IMP-EV-0212 — No fake test examples

- **Requirement:** `REQ-EV-0212`
- **Disposition:** ADOPT
- **Mandatory behavior:** Examples used for evaluation must be executable/verified, not illustrative placeholders.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0212` — Docs/example runner executes declared examples and fails release on drift.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Reliability Layer

### IMP-EV-0073 — Typed failure/recovery/operator diagnostics

- **Requirement:** `REQ-EV-0073`
- **Disposition:** ADOPT
- **Mandatory behavior:** Failures carry class, retryability, user action, evidence and recovery path.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0073` — Fault injection verifies no generic success on timeout/corrupt state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0245 — Structured failure diagnostics

- **Requirement:** `REQ-EV-0245`
- **Disposition:** ADOPT
- **Mandatory behavior:** Adaptive evaluator receives typed failure taxonomy/evidence, not raw guess.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0245` — Fault corpus produces stable diagnostic features.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Repository Index

### IMP-EV-0004 — Incremental Merkle repository indexing

- **Requirement:** `REQ-EV-0004`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use subtree/content identity to avoid full rebuilds and bind index state to repo revision.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0004` — Large-repo incremental edit updates only affected index segments and stays revision-correct.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0172 — Incremental large-codebase indexing

- **Requirement:** `REQ-EV-0172`
- **Disposition:** ADOPT
- **Mandatory behavior:** Update lexical/symbol/semantic/graph indices incrementally.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0172` — Cold vs incremental index benchmarks reported.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Repository Knowledge

### IMP-EV-0060 — Repository Knowledge Artifact / Wiki

- **Requirement:** `REQ-EV-0060`
- **Disposition:** ADAPT
- **Mandatory behavior:** Generated architecture summaries are cache/discovery aids and always source-checked.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0060` — Edit source after wiki generation; stale claim flagged and never treated as authority.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Resource Governor

### IMP-EV-0272 — Capacity tickets

- **Requirement:** `REQ-EV-0272`
- **Disposition:** ADOPT
- **Mandatory behavior:** Concurrency admission consumes explicit tenant/run resource ticket before spawn/sandbox.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0272` — Capacity exhaustion denies launch without partial side effects.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Sandbox Gateway

### IMP-EV-0286 — Authenticated tenant-bound Sandbox Gateway

- **Requirement:** `REQ-EV-0286`
- **Disposition:** ADOPT
- **Mandatory behavior:** Every guest lifecycle/RPC request authenticates tenant/run/workspace capability.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0286` — Cross-tenant sandbox handle use is denied and audited.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Sandbox Policy

### IMP-EV-0287 — Deny-by-default sandbox/network isolation

- **Requirement:** `REQ-EV-0287`
- **Disposition:** ADOPT
- **Mandatory behavior:** Guest starts with bounded fs/network/resource policy; explicit grants only.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0287` — Guest cannot reach internal/control-plane endpoints by default.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0290 — Fine-grained filesystem/protected-path enforcement

- **Requirement:** `REQ-EV-0290`
- **Disposition:** ADOPT
- **Mandatory behavior:** Guest writes restricted by workspace/protected paths and task capability.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0290` — Attempt protected host/control path write fails.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## SandboxBackend

### IMP-EV-0285 — Common cloud Linux MicroVM substrate

- **Requirement:** `REQ-EV-0285`
- **Disposition:** ADOPT
- **Mandatory behavior:** Use a hardened MicroVM-class isolated guest behind Modbit-owned abstraction/gateway.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0285` — Real cloud sandbox boots fixture and passes backend conformance.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Scheduler

### IMP-EV-0180 — Default background delegation policy

- **Requirement:** `REQ-EV-0180`
- **Disposition:** ADAPT
- **Mandatory behavior:** Background only when dependency graph says parent can progress independently; never blind default.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0180` — Dependency-sensitive scheduler keeps blocking child foreground and separable child background.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Secret Broker

### IMP-EV-0215 — Tool credentials outside tool arguments

- **Requirement:** `REQ-EV-0215`
- **Disposition:** ADOPT
- **Mandatory behavior:** Required/optional secrets resolved by host, never exposed as model parameter values.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0215` — Schema inspection contains no API key field; secret redaction test passes.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0288 — Dynamic credential handles / broker injection

- **Requirement:** `REQ-EV-0288`
- **Disposition:** ADOPT
- **Mandatory behavior:** Guest receives short-lived scoped secret handle/injection, not static secrets in image/config.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0288` — Inspect guest image/env/artifacts; long-lived provider secret absent.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Semantic Browser Compiler

### IMP-EV-0277 — Accessibility/DOM/CDP semantic state

- **Requirement:** `REQ-EV-0277`
- **Disposition:** ADOPT
- **Mandatory behavior:** Fuse AX/DOM/layout/network state into compact model-facing page representation.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0277` — Accessible workflow completes without screenshot/OCR dependency.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0278 — Stable semantic element references

- **Requirement:** `REQ-EV-0278`
- **Disposition:** ADAPT
- **Mandatory behavior:** References are scoped to browser-state version and invalidate safely on relevant change.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0278` — DOM mutation makes stale ref return TARGET_STALE, never click wrong element.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Session Store

### IMP-EV-0054 — SessionLease + fencing generation

- **Requirement:** `REQ-EV-0054`
- **Disposition:** ADOPT
- **Mandatory behavior:** Single mutation owner with lease generation; stale writer rejected across desktop/cloud/CLI.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0054` — Simulate dual resume; only current lease can append mutation events.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0121 — Resume/sessions

- **Requirement:** `REQ-EV-0121`
- **Disposition:** ADOPT
- **Mandatory behavior:** Resume from canonical state/event cursor.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0121` — Resume after Core crash reproduces pending state exactly.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0273 — Kernel/session lease locks

- **Requirement:** `REQ-EV-0273`
- **Disposition:** ADOPT
- **Mandatory behavior:** Lease/fencing prevents dual active mutation owners.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0273` — Two clients attempt mutation; stale lease rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill Compiler

### IMP-EV-0061 — Skill execution capsule

- **Requirement:** `REQ-EV-0061`
- **Disposition:** ADOPT
- **Mandatory behavior:** Skill declares invocation contract, context, tool requirements, model policy and verification; capability ceiling only narrows.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0061` — Malicious skill requesting admin capability cannot widen task authority.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0213 — Compact skill mode / selective context

- **Requirement:** `REQ-EV-0213`
- **Disposition:** ADAPT
- **Mandatory behavior:** Only task-relevant skill instructions/resources enter prompt; large references lazy-load.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0213` — Token benchmark compares eager package vs compiled skill projection.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0230 — Procedural skills over existing tools

- **Requirement:** `REQ-EV-0230`
- **Disposition:** ADOPT
- **Mandatory behavior:** Prefer skill instructions/scripts when existing canonical tools suffice; build native tool only for precise new effector/auth/streaming need.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0230` — Build/buy lint requires justification for every new tool namespace.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill Evolution Lab

### IMP-EV-0196 — Separate raw experience / persistent wiki / executable skills

- **Requirement:** `REQ-EV-0196`
- **Disposition:** ADAPT
- **Mandatory behavior:** Keep immutable evaluation traces, persistent evolution knowledge and candidate skill packages as distinct stores.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0196` — Delete/reject candidate skill; raw traces and wiki knowledge remain intact.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0197 — Persistent wiki across iterations

- **Requirement:** `REQ-EV-0197`
- **Disposition:** ADAPT
- **Mandatory behavior:** Evolution knowledge is versioned, provenance-bound and persists independently of active skill version.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0197` — Rollback candidate and verify wiki head unchanged unless separately reverted.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0204 — On-demand proposer retrieval from wiki index

- **Requirement:** `REQ-EV-0204`
- **Disposition:** ADAPT
- **Mandatory behavior:** Evolution agent starts with compact index/outcome summaries and hydrates specific patterns/traces.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0204` — Large evolution corpus stays within token budget and provenance remains complete.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill Package

### IMP-EV-0202 — PURPOSE / motivating knowledge linkage

- **Requirement:** `REQ-EV-0202`
- **Disposition:** ADAPT
- **Mandatory behavior:** Skill metadata links purpose, assumptions and evidence without dumping wiki into runtime prompt.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0202` — Runtime loads purpose summary; detailed evolution wiki remains inaccessible by default.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0209 — SKILL.md-based procedural packaging

- **Requirement:** `REQ-EV-0209`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use portable skill documentation/resources while keeping Modbit execution policy separate.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0209` — Package parser validates metadata/resources and rejects malformed/oversized package.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill Registry

### IMP-EV-0114 — Filesystem-discovered skills

- **Requirement:** `REQ-EV-0114`
- **Disposition:** ADOPT
- **Mandatory behavior:** Discover governed personal/project skills with SKILL.md-like portable format.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0114` — Add/remove skill on disk; registry refreshes with hash/provenance and invalid metadata fails.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0181 — Extension-provided skills

- **Requirement:** `REQ-EV-0181`
- **Disposition:** ADOPT
- **Mandatory behavior:** Import/discover portable SKILL.md packages with provenance and trust.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0181` — Install extension skill, validate hash, activate without capability escalation.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0201 — Skill impact log

- **Requirement:** `REQ-EV-0201`
- **Disposition:** ADOPT
- **Mandatory behavior:** Record proposal diff, source patterns, benchmark scores, disposition, model and environment.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0201` — Audit can reconstruct why each skill version was accepted/rejected.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0214 — Disable model invocation metadata

- **Requirement:** `REQ-EV-0214`
- **Disposition:** ADAPT
- **Mandatory behavior:** Skill can be user-only/system-only/model-invocable according to policy.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0214` — Model cannot invoke a skill marked non-model-invocable.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill Registry + Eval

### IMP-EV-0200 — Validation gating and rollback

- **Requirement:** `REQ-EV-0200`
- **Disposition:** ADOPT
- **Mandatory behavior:** No evolved skill becomes active until benchmark gates pass; rejected candidate is retained as evaluation artifact.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0200` — Candidate regressing safety/quality is rejected and previous active skill remains byte-identical.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Skill/Tool Developer Kit

### IMP-EV-0210 — Tool/skill creation with registration validation

- **Requirement:** `REQ-EV-0210`
- **Disposition:** ADOPT
- **Mandatory behavior:** New tool/skill must validate schema, registry wiring and invocation path.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0210` — Test plugin registers, lists, invokes real effector and passes removal/reload.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Task Isolation Bundle

### IMP-EV-0145 — Task→branch/environment isolation

- **Requirement:** `REQ-EV-0145`
- **Disposition:** ADOPT
- **Mandatory behavior:** Bind worktree, sandbox, context, lease, credentials, capability snapshot and evidence namespace.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0145` — Parallel tasks prove isolation across every bound resource.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Task Runtime

### IMP-EV-0119 — Goal mode

- **Requirement:** `REQ-EV-0119`
- **Disposition:** ADAPT
- **Mandatory behavior:** Persistent objective/progress/termination are host-owned; model cannot self-certify.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0119` — Model says done while acceptance fails; run remains incomplete.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0221 — Background task list/output/stop

- **Requirement:** `REQ-EV-0221`
- **Disposition:** ADOPT
- **Mandatory behavior:** Background operations have durable handle, status, bounded preview, full OutputRef and cancel.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0221` — Run long task, list/status/read full output/stop after UI restart.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Terminal Broker

### IMP-EV-0019 — Terminal scrollback as artifact

- **Requirement:** `REQ-EV-0019`
- **Disposition:** ADOPT
- **Mandatory behavior:** Large terminal history becomes bounded OutputRef/artifact, not prompt dump.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0019` — Generate >10MB output; UI can replay, model receives bounded view, full artifact remains retrievable.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0025 — Stateless process / stateful shell execution model

- **Requirement:** `REQ-EV-0025`
- **Disposition:** ADAPT
- **Mandatory behavior:** Commands may be one-shot while optional durable shell sessions preserve explicit cwd/env.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0025` — Two concurrent workspaces cannot leak cwd/env or aliases.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0026 — Layered terminal output economics

- **Requirement:** `REQ-EV-0026`
- **Disposition:** ADOPT
- **Mandatory behavior:** Stream → batch → bounded model view → full OutputRef; repeated noise suppression is reversible.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0026` — Noisy build shows token reduction while raw output digest remains complete.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0027 — Long command foreground→background

- **Requirement:** `REQ-EV-0027`
- **Disposition:** ADOPT
- **Mandatory behavior:** Detach long process to durable handle without orphaning or losing cursor.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0027` — Start long test, detach, restart UI, reattach and cancel successfully.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0100 — Structured command contract

- **Requirement:** `REQ-EV-0100`
- **Disposition:** ADOPT
- **Mandatory behavior:** argv/cwd/env/timeout/PTY/output budget/stream/cancel are explicit.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0100` — Conformance suite exercises each field against real processes.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0135 — Background shell handles + bounded output

- **Requirement:** `REQ-EV-0135`
- **Disposition:** ADOPT
- **Mandatory behavior:** Long commands detach to handles; full logs via OutputRef.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0135` — Background process survives UI restart and output cap.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0271 — Durable terminal + replay window

- **Requirement:** `REQ-EV-0271`
- **Disposition:** ADOPT
- **Mandatory behavior:** PTY/process output survives client disconnect with cursor replay/backpressure.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0271` — Restart desktop, replay exact terminal tail and continue input.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Tool Registry

### IMP-EV-0134 — Deferred tool search

- **Requirement:** `REQ-EV-0134`
- **Disposition:** ADOPT
- **Mandatory behavior:** Stable core + searchable deferred metadata; discovery does not authorize.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0134` — Search tool catalog then activate; permission still enforced.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0229 — Toolsets / capability grouping

- **Requirement:** `REQ-EV-0229`
- **Disposition:** ADAPT
- **Mandatory behavior:** Group discoverable tools for projection but resolve authority in Capability Kernel.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0229` — Toolset enablement cannot expose denied tool.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Tool Runtime

### IMP-EV-0079 — Typed tools

- **Requirement:** `REQ-EV-0079`
- **Disposition:** ADOPT
- **Mandatory behavior:** Schemas normalize intent before policy and execution.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0079` — Invalid alias/schema is repaired or rejected before effector.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0096 — Configuration-dependent model tool surface

- **Requirement:** `REQ-EV-0096`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compile visible tools per task/turn from support × policy × relevance.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0096` — Snapshot tool schemas across modes and verify denied/irrelevant tools absent.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0116 — Minimal tool sets per agent

- **Requirement:** `REQ-EV-0116`
- **Disposition:** ADOPT
- **Mandatory behavior:** Project smallest supported/authorized/relevant surface.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0116` — Tool-schema token benchmark vs eager all-tools baseline.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0177 — Lazy tool/schema context

- **Requirement:** `REQ-EV-0177`
- **Disposition:** ADOPT
- **Mandatory behavior:** Hydrate schemas only when relevant after discovery; authorization separate.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0177` — Large MCP catalog token benchmark proves lazy behavior.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0217 — Explicit built-in tool families

- **Requirement:** `REQ-EV-0217`
- **Disposition:** ADAPT
- **Mandatory behavior:** Map file/shell/search/web/user-question/plan/task/agent/media behaviors into canonical Modbit tools, not source names.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0217` — Compatibility matrix has canonical owner/effect/test for each source capability.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0239 — Typed tool pre/execute/post pipeline

- **Requirement:** `REQ-EV-0239`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use canonical validation/policy/execute/postprocess/evidence stages with monotonic final guards.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0239` — Hook tries to override deny after guard; execution remains denied.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Transport

### IMP-EV-0108 — Bounded IPC dispatch/chunking

- **Requirement:** `REQ-EV-0108`
- **Disposition:** ADOPT
- **Mandatory behavior:** Large payloads stream/chunk or use OutputRef; no giant IPC body.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0108` — Multi-MB terminal/browser result remains responsive and memory-bounded.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Usage Ledger

### IMP-EV-0032 — Per-run token/cost accounting

- **Requirement:** `REQ-EV-0032`
- **Disposition:** ADOPT
- **Mandatory behavior:** Attribute provider usage and verification/tool overhead to run/step.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0032` — Reconcile provider invoice sample against canonical usage events within tolerance.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Verification Plane

### IMP-EV-0018 — Regression-only diagnostics comparison

- **Requirement:** `REQ-EV-0018`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compare pre/post diagnostics and attribute only introduced regressions when appropriate.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0018` — Fixture with pre-existing errors proves baseline issues are not blamed on change.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0068 — VerificationPlane

- **Requirement:** `REQ-EV-0068`
- **Disposition:** ADOPT
- **Mandatory behavior:** Compose deterministic, evidence, change, environment and optional semantic verifiers.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0068` — Verifier crash yields INDETERMINATE, never success.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0070 — Diagnostic Change Window

- **Requirement:** `REQ-EV-0070`
- **Disposition:** ADOPT
- **Mandatory behavior:** Capture baseline before mutation and bounded post-change diagnostics for changed regions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0070` — High-noise repo proves only relevant post-change window is evaluated.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0071 — PatchPolicyGate

- **Requirement:** `REQ-EV-0071`
- **Disposition:** ADAPT
- **Mandatory behavior:** Run security/license/secret/static/test gates before commit/merge as policy requires.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0071` — Seed secret in patch; merge blocked with evidence.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## WorkGraph

### IMP-EV-0052 — Structured PlanGraph/TodoState

- **Requirement:** `REQ-EV-0052`
- **Disposition:** ADOPT
- **Mandatory behavior:** Plan nodes exist outside transcript with dependencies/owner/status/evidence/blockers.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0052` — Compaction and model restart cannot alter canonical plan state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0120 — Todo/tasklist

- **Requirement:** `REQ-EV-0120`
- **Disposition:** ADOPT
- **Mandatory behavior:** Durable dependency task graph with evidence and attempts.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0120` — Restart preserves task statuses independent of chat compaction.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## WorkGraph UI

### IMP-EV-0118 — Saved plan browser/review/annotation

- **Requirement:** `REQ-EV-0118`
- **Disposition:** ADOPT
- **Mandatory behavior:** Plan versions live outside transcript and support review/annotations.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0118` — Edit/review plan then resume; exact version ID is recorded in execution.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Worker Fabric

### IMP-EV-0075 — Out-of-process privileged execution

- **Requirement:** `REQ-EV-0075`
- **Disposition:** ADOPT
- **Mandatory behavior:** Native/browser/computer privileged effects execute outside renderer/model behind typed RPC.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0075` — Renderer compromise test cannot invoke privileged effect without capability token.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0081 — Dedicated browser/viewer/computer workers

- **Requirement:** `REQ-EV-0081`
- **Disposition:** ADAPT
- **Mandatory behavior:** Specialized surfaces use same canonical run/evidence contracts.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0081` — Browser worker crash does not corrupt Core and restart reattaches session.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Worker Protocol

### IMP-EV-0072 — Bidirectional capability negotiation

- **Requirement:** `REQ-EV-0072`
- **Disposition:** ADOPT
- **Mandatory behavior:** Core and worker negotiate supported protocols/tools/media/runtime features before task dispatch.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0072` — Older worker missing capability receives compatible task projection or explicit rejection.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace Browser Surface

### IMP-EV-0283 — Same live browser session user observe/takeover

- **Requirement:** `REQ-EV-0283`
- **Disposition:** ADOPT
- **Mandatory behavior:** Dock/expand/pop-out/remote viewer share one session; human takeover revokes automation controller.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0283` — User takes over mid-run with no browser restart and agent resumes after reacquisition.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace Context Bridge

### IMP-EV-0141 — Editor context bridge

- **Requirement:** `REQ-EV-0141`
- **Disposition:** ADAPT
- **Mandatory behavior:** In clean-slate Modbit this means selected/open review artifacts and active file/symbol context, not IDE ownership.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0141` — Review selection affects context but cannot mutate canonical source.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0160 — Editor/active state context

- **Requirement:** `REQ-EV-0160`
- **Disposition:** ADAPT
- **Mandatory behavior:** Use active review/file/symbol/task selection without needing an IDE.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0160` — Selection influences retrieval and is visible in inspector.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace Fabric

### IMP-EV-0021 — Environment source hierarchy

- **Requirement:** `REQ-EV-0021`
- **Disposition:** ADAPT
- **Mandatory behavior:** Version repo/team/user environment inputs with explicit precedence and staleness state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0021` — Change environment definition and verify run pins old revision until explicit rebuild.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0022 — Git snapshot local→cloud handoff

- **Requirement:** `REQ-EV-0022`
- **Disposition:** ADAPT
- **Mandatory behavior:** Represent dirty local state with provenance-bound Git tree/temporary commit where possible.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0022` — Cloud run reconstructs dirty state exactly and cleanup removes temporary refs safely.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0062 — EnvironmentSnapshot / Revision

- **Requirement:** `REQ-EV-0062`
- **Disposition:** ADOPT
- **Mandatory behavior:** Pin toolchain/PATH/env refs/workspace roots/tool availability to revision identity.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0062` — Resume detects unavailable environment revision and follows explicit rebuild/fail path.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0063 — EnvironmentHandoffBundle

- **Requirement:** `REQ-EV-0063`
- **Disposition:** ADOPT
- **Mandatory behavior:** Transfer task/plan/context/evidence/git delta/runtime requirements/secret refs after capability negotiation.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0063` — Local→cloud handoff preserves state but never embeds secret values.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0124 — Clone branch into new session

- **Requirement:** `REQ-EV-0124`
- **Disposition:** ADOPT
- **Mandatory behavior:** New task/run binds isolated branch/worktree with lineage.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0124` — Two sessions modify separately and merge transaction detects conflict.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0146 — Blueprints/environment snapshots

- **Requirement:** `REQ-EV-0146`
- **Disposition:** ADOPT
- **Mandatory behavior:** Reusable revisioned environment definitions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0146` — Rebuild and pin exact environment digest.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0176 — Workspace Capsule

- **Requirement:** `REQ-EV-0176`
- **Disposition:** ADOPT
- **Mandatory behavior:** Portable bounded task package of context/decisions/evidence/runtime requirements.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0176` — Handoff verifies no authority/secret values smuggled in capsule.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace UI

### IMP-EV-0035 — Context Inspector

- **Requirement:** `REQ-EV-0035`
- **Disposition:** ADOPT
- **Mandatory behavior:** Expose selected files/symbols/reasons/revision/token cost and exclusions.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0035` — E2E task compares UI inspector against actual PromptEnvelope context IDs.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0037 — Attention/fleet supervision

- **Requirement:** `REQ-EV-0037`
- **Disposition:** ADAPT
- **Mandatory behavior:** Fleet is projection of canonical run/task state, not second runtime.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0037` — State transitions update attention buckets after desktop reconnect with no client-local truth.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0143 — Agent Fleet / status center

- **Requirement:** `REQ-EV-0143`
- **Disposition:** ADOPT
- **Mandatory behavior:** Attention buckets project canonical run state.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0143` — Reconnect test verifies buckets from Core state.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0152 — Task-centric command center

- **Requirement:** `REQ-EV-0152`
- **Disposition:** ADOPT
- **Mandatory behavior:** Supervise multiple runs without a second runtime.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0152` — UI reload derives all task state from Core APIs.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

### IMP-EV-0175 — Context Inspector

- **Requirement:** `REQ-EV-0175`
- **Disposition:** ADOPT
- **Mandatory behavior:** User sees what context selected/excluded and why.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0175` — Inspector ids match PromptEnvelope.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace UI + Change Engine

### IMP-EV-0036 — Per-hunk diff review

- **Requirement:** `REQ-EV-0036`
- **Disposition:** ADOPT
- **Mandatory behavior:** Review applies/rejects revision-bound hunks with provenance and evidence.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0036` — Reject one hunk and accept another; resulting Git diff matches user choices exactly.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Workspace UX

### IMP-EV-0265 — Q&A → Plan → visual review

- **Requirement:** `REQ-EV-0265`
- **Disposition:** ADAPT
- **Mandatory behavior:** Risk/ambiguity may trigger clarification, plan and visual evidence review without mandatory ceremony.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0265` — Low-risk task skips ceremony; high-risk configured task requires plan/review.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Worktree Manager

### IMP-EV-0125 — Git worktree commands

- **Requirement:** `REQ-EV-0125`
- **Disposition:** ADOPT
- **Mandatory behavior:** First-class isolated writes and reviewed merge.
- **Existing-code audit:** trace production caller → owner → policy/persistence → real effect/evidence before changing code.
- **Acceptance:** `QUAL-EV-0125` — Parallel E2E verifies no cross-worktree writes.
- **Completion:** production wiring + real qualification + failure behavior + retained evidence.

## Experiments — not production commitments

### IMP-EV-0024 — Private worker reverse-connect

- **Requirement:** `REQ-EV-0024`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Evaluate outbound worker connection for NAT/private networks behind same authenticated worker protocol.
- **Qualification:** `QUAL-EV-0024` — VPC test with no inbound ports proves identity, reconnect, revocation and tenant isolation.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0069 — Optional adaptive LLM verification

- **Requirement:** `REQ-EV-0069`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Semantic verifier is optional/off by default and subordinate to deterministic gates.
- **Qualification:** `QUAL-EV-0069` — Disabled configuration emits zero verifier model calls.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0198 — Wiki Maintainer consolidation

- **Requirement:** `REQ-EV-0198`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Offline/eval worker consolidates success/failure traces into patterns; never production authority.
- **Qualification:** `QUAL-EV-0198` — Seed contradictory traces; maintainer records both with provenance/confidence rather than overwriting.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0199 — Skill Proposer from wiki + traces

- **Requirement:** `REQ-EV-0199`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Generate atomic candidate skill diffs from selected patterns/traces and explicit objective.
- **Qualification:** `QUAL-EV-0199` — Candidate diff references motivating evidence IDs and changes one bounded behavior.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0205 — Cross-model skill transfer evaluation

- **Requirement:** `REQ-EV-0205`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Evaluate evolved skill on source and at least one distinct model family before broad promotion where economically feasible.
- **Qualification:** `QUAL-EV-0205` — Nightly matrix reports baseline vs skill deltas per model and rejects hidden regression.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0206 — Skill evolution complements model scaling hypothesis

- **Requirement:** `REQ-EV-0206`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Treat paper result as research hypothesis, not Modbit claim; measure on engineering tasks.
- **Qualification:** `QUAL-EV-0206` — A/B benchmark uses same tasks/environment and records confidence intervals.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0207 — Persistent knowledge is critical hypothesis

- **Requirement:** `REQ-EV-0207`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Run ablation without wiki/persistent evolution knowledge to justify added complexity.
- **Qualification:** `QUAL-EV-0207` — Promotion of evolution-lab mechanism requires statistically/practically meaningful lift vs simpler skill refinement.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0237 — Self-improving skills

- **Requirement:** `REQ-EV-0237`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Evaluate only through evolution-lab-style trace/wiki/candidate/eval gates; no autonomous production self-modification.
- **Qualification:** `QUAL-EV-0237` — Skill cannot self-promote without eval/promotion transaction.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0244 — Task-conditioned harness generation

- **Requirement:** `REQ-EV-0244`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Generate declarative profile variants only in shadow/eval initially.
- **Qualification:** `QUAL-EV-0244` — Shadow candidate never controls production run.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0246 — Bounded repair of harness/profile

- **Requirement:** `REQ-EV-0246`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Max two bounded profile repairs; static known-good fallback always available.
- **Qualification:** `QUAL-EV-0246` — Third repair attempt rejected; fallback run remains functional.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0247 — Archive/evolution of profiles

- **Requirement:** `REQ-EV-0247`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Version candidate profiles with benchmark outcomes and rollback.
- **Qualification:** `QUAL-EV-0247` — Rejected candidate remains audit artifact but never active.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0248 — Optimize reward/latency/cost jointly

- **Requirement:** `REQ-EV-0248`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Promotion requires correctness/safety hard gates before economics.
- **Qualification:** `QUAL-EV-0248` — Cheap but lower-correctness profile cannot promote.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

### IMP-EV-0254 — Modbit structural advantage hypothesis

- **Requirement:** `REQ-EV-0254`
- **Entry criterion:** isolated behind existing canonical owner; no second subsystem.
- **Hypothesis:** Test AST/symbol/call/dependency/Git/test signals over hybrid-only baseline.
- **Qualification:** `QUAL-EV-0254` — Profile A baseline, B hybrid, C structural with paired trials.
- **Exit:** promote only through ADR + measurable benefit; otherwise remove cleanly.

## EPR additive implementation work

The 265 IMP-EV tasks remain unchanged. EPR-000..019 in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` extend the existing owners under DR-EPR-2026-09-05-v1.1. When working IMP-EV-0029/0030/0112/0189/0256, load the related EPR contracts and qualification in docs 27/38/49/61; complete existing behavior in place rather than introducing another router/runtime. All task states remain in the same project graph.
