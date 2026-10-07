# Architectural Conflicts and Supersessions

> **Purpose:** prevent historical project authority from silently re-entering the clean-slate Modbit build.

## Authority rule

Latest explicit user decision wins over older locks. A historical **LOCKED** row does not remain normative when a later explicit product decision replaces its premise. The mechanism knowledge may survive even when the old component is retired.

| Historical decision / assumption | Current disposition | Resolution | What survives |
|---|---|---|---|
| Code-OSS-derived desktop is foundational | **SUPERSEDED / REPLACED** | Clean-slate Modbit has no Code-OSS runtime dependency | Git/worktree, diagnostics, diff, IPC and workflow lessons only |
| Full IDE is primary product surface | **SUPERSEDED / REPLACED** | Agent-first Work + Code workspace | revision-bound code review/inspection; no full editor ownership |
| Monaco/native editor as new shell | **REJECTED** | Do not build a general editor | code rendering/diff components only |
| Separate Modbit Lite | **SUPERSEDED / RETIRED** | One Modbit product | durable-state/sandbox lessons merge into Modbit |
| Code-OSS fallback inside main product | **REJECTED** | Any future editor adapter is external/optional and cannot own Core | protocol compatibility only |
| Firecracker-specific cloud backend as architecture | **SUPERSEDED** | Current substrate is a hardened MicroVM-class sandbox behind the Modbit Sandbox Gateway | replaceable SandboxBackend interface |
| Memory can represent resumed session truth | **REJECTED** | Memory is not recovery | Engineering Memory remains separate curated layer |
| General local SLM runtime as product subsystem | **REJECTED / CANCELLED** | No production SLM subsystem | non-generative embeddings or explicit provider vision bridge are infrastructure, not local reasoning authority |
| Vector search as main/sole retrieval | **REJECTED** | Hybrid + structural retrieval planner | vector signal remains one candidate source |
| Eager expose all tools | **REJECTED** | task-scoped projection + deferred tool tail | full registry remains host-side |
| Unbounded parallel agents | **REJECTED** | transactional admission, capacity, worktree/conflict controls | bounded parallelism for separable work |
| Mandatory LLM judge on every task | **REJECTED** | deterministic/evidence verification baseline | optional semantic verifier remains eval/policy-gated |
| External reference code/service dependency | **REJECTED** | clean-room reimplementation of mechanisms | source provenance and benchmark references |
| Evolution-lab research as new general memory/runtime | **REJECTED FORM** | Skill Evolution Lab is isolated evaluation subsystem under existing Skill Registry/Eval architecture | trace/wiki/skill separation and gated promotion |
| External-reference tool namespaces as canonical | **REJECTED FORM** | normalize to Modbit tool intent/effect contracts | capabilities and conformance tests |

## Retain / modify / replace / reconsider calls

- **RETAIN:** Core-authoritative state, WorkGraph/AgentGraph/StateGraph, Capability Kernel, Context Engine, Effect/Evidence Ledger, durable terminal, live browser, worktree isolation and real E2E completion semantics.
- **MODIFY:** skills now include an eval-gated evolution pipeline; file/tool results now have a first-class MediaEnvelope; input steering is explicit `STEER/COLLECT/FOLLOW_UP`.
- **REPLACE:** all Code-OSS/IDE-foundation decisions with the clean-slate Work + Code shell and Core APIs.
- **RECONSIDER only with evidence:** adaptive/JIT profile generation, cross-model skill evolution defaults, private-worker reverse connect and enterprise TLS interception.

## Migration rule for old implementation code

Old source is **donor material only**. To enter the new repository it must be architecture-independent, license-clean, covered by tests, free of Code-OSS assumptions at its boundary and conform to the new canonical contracts. Existing implementation does not inherit old authority simply because it already exists.

## Execution policy supersessions (DR-EPR-2026-09-05)

| Historical rule | Disposition | Active replacement |
|---|---|---|
| Frontier LLM is sole production reasoning intelligence | **SUPERSEDED BY ADR-R-039..048** | Eligible provider-neutral reasoning roles; frontier is quality backstop, not mandatory first hop |
| Frontier-model tokens per verified outcome is primary efficiency metric | **SUPERSEDED BY ADR-R-045** | Total inference cost, human intervention and wall-clock per verified success, subject to quality floor |
| TaskFingerprint mixes task demand with provider/economic/policy features | **REPLACED by ADR-R-040/041** | RequestProfile contains intrinsic task features; PolicyEnvelope and compiler join versioned external facts |
| Auto always displays selected model/provider | **AMENDED by DR-EPR-2026-09-05** | Objective modes and useful progress; model labels policy-controlled, internal routing audit always retained |

No historical ADR IDs are invented for absent rows. The earlier rejection of a general local SLM subsystem means no mandatory local reasoning dependency; an optional eligible private/local endpoint still uses the existing Gateway. Approved bounded workflow selection is distinct from MOD-JIT-001 arbitrary task-conditioned harness generation, which remains EXPERIMENT. No mandatory LLM judge or hidden fixed hierarchy is introduced.

## EPR v1.1 surgical supersession

DR-EPR-2026-09-05-v1.1 and ADR-R-049..056 replace only the conflicting v1.0 template, soft utility, registry-empirical, conflated risk/acceptance, absolute reviewer tool prohibition and runtime branch-recompilation clauses. Active replacements and migration are in docs 06/27/38. DIRECT/CASCADE/CRITIQUE now label observed paths; Outcome Statistics is separate versioned derived data; reviewers may gather evidence through bounded disposable execution with no canonical/persistent/external effects. Model-independent profiling, one Core, protected effects, real verification and controlled rollback survive. All original clauses remain identifiable in the immutable root v1.0 patch and historical doc 05/94 evidence.

## Parity-push supersessions (DR-PX-2026-10-03-008 to -012; -008 and -010 accepted 2026-10-05, -009, -011 and -012 proposed)

The rows citing DR-PX-2026-10-03-008 and -010 are in force from 2026-10-05 (owner instruction, goal: implement research/audit/01-TASK-LIST.md) and only in the scope stated. The rows citing -009, -011 and -012 stay proposed and take effect only when their record is accepted; until then the historical row stays as it is. Nothing here changes a pinned count, an EPR clause or a locked requirement row; where a base requirement row is affected the additive PX rows take over and the base row stays byte-identical.

| Historical decision / assumption | Current disposition | Resolution | What survives |
|---|---|---|---|
| doc 10 non-goal: pixel-only computer-use product | **CLARIFIED, NOT SUPERSEDED** (DR-PX-2026-10-03-008) | Native control is semantic-first (accessibility tree, then coordinates against a fresh observation, then raw input). A pixel-only product stays a non-goal. | The non-goal as written; doc 22's locked direction that Modbit does not operate primarily from screenshots. |
| REQ-EV-0083 to REQ-EV-0090 (ADOPT, Computer Runtime) | **FULFILLED, NOT CHANGED** (DR-PX-2026-10-03-008) | The rows already require exact application identity, a single controller lock, a watchdog, human preemption and a typed failure taxonomy; PX-069 to PX-076 implement them for native applications. | Every row text, disposition and owner. |
| doc 17 `computer.observe` and `computer.action` rows | **IMPLEMENTED BY** (DR-PX-2026-10-03-008) | The tool family of PX-069 registers under the existing Computer Runtime owner (the `browser` subsystem). | The rows. |
| MOD-IDE-002 (REJECTED): build a new general-purpose code editor | **SUPERSEDED IN SCOPE** (DR-PX-2026-10-03-009) | A scoped Workspace Editor (a transactional editing surface that saves only through ChangeTransaction) is admitted. A general-purpose editor stays rejected. | The rejection of a general-purpose editor, Code-OSS and Monaco. |
| MOD-SURF-002 (LOCKED): no built-in full IDE or Monaco architecture; code shown through trusted review surfaces; editing is not the primary UI | **SUPERSEDED IN SCOPE** (DR-PX-2026-10-03-009) | Editing becomes a secondary surface with a draft overlay. Monaco, an IDE architecture, an extension host and editing as the primary UI remain prohibited. | No Monaco, no IDE, review as the primary judgement surface. |
| doc 20: UI buffers are never canonical; the surface does not own unsaved editor buffers; P0 does not build a general editor | **SUPERSEDED IN SCOPE** (DR-PX-2026-10-03-009) | The draft overlay of WED-A02 is volatile, non-authoritative, rebased or discarded on a moved base and read by no Core component; filesystem plus Git revision stay canonical. | Canonicality of the filesystem and Git revision. |
| doc 29: an embedded editor remains a non-goal | **SUPERSEDED IN SCOPE (editor); RETAINED (Tab completion, replacing an IDE's language features)** (DR-PX-2026-10-03-009) | The editor is admitted; completion lists, signature help and predictive completion stay out (WED-C03). | The non-goals for completion and language-feature replacement. |
| doc 32: no Monaco, Code-OSS or editor-buffer architecture | **SUPERSEDED IN SCOPE (buffer); RETAINED (Monaco, Code-OSS)** (DR-PX-2026-10-03-009) | A draft overlay replaces the editor-buffer ban for the Workspace Editor only. | No Monaco, no Code-OSS. |
| PX-005 qualification: no client keeps an unsaved buffer | **SUPERSEDED IN SCOPE** (DR-PX-2026-10-03-009) | Applies to the single-hunk inline patch; the Workspace Editor's drafts are the stated exception (WED-A02). | The inline patch path and its qualification. |
| MOD-SURF-001 (LOCKED, no Code-OSS foundation) and MOD-IDE-001 (REJECTED, Code-OSS fallback) | **UNCHANGED** (DR-PX-2026-10-03-009) | Nothing in this record uses or depends on Code-OSS. | Both rows. |
| MOD-AUTO-001 (DEFERRED): broad consumer automations and scheduling | **SUPERSEDED IN SCOPE** (DR-PX-2026-10-03-010) | Software-engineering automations (schedule, forge events, signed webhooks, manual) are adopted through the existing Scheduler. Broad consumer automations stay out. | Consumer and messaging breadth stays out (REQ-EV-0236). |
| REQ-EV-0149 (DEFERRED, cron, event and webhook triggers) and REQ-EV-0264 (DEFERRED, recurring goals and watch) | **ADOPTED BY ADDITIVE ROWS** (DR-PX-2026-10-03-010) | The sealed base rows stay byte-identical and DEFERRED; REQ-PX-082 to REQ-PX-086 are the additive adoption and carry the same constraints (explicit principal, budget and policy; the model never self-grants scheduling authority). | Row text, disposition and owner of both base rows. |
| `automation` subsystem (DEFERRED, no crate, no milestone) | **ACTIVATED IN SCOPE** (DR-PX-2026-10-03-010) | It owns definitions only. The Scheduler, policy kernel, approval aggregate and effect ledger keep their owners. | doc 81 single-owner list. |
| doc 10 non-goal: a marketplace-driven architecture in P0 | **CLARIFIED** (DR-PX-2026-10-03-011) | The non-goal's intent holds: the Core and every task run with no catalog reachable. A marketplace is a client-side feature on the existing extension system, added after P0. | Core independence from any marketplace. |
| REQ-EV-0225 (ADOPT, marketplace trust surfaced) and REQ-EV-0138 (ADAPT, unified plugins) | **EXTENDED, NOT CHANGED** (DR-PX-2026-10-03-011) | PX-087 to PX-093 extend the existing package, trust and quarantine model. | Row text, disposition and owner. |
| REQ-EV-0216 (REJECT, adopting an external skill-packaging framework) | **UNCHANGED** (DR-PX-2026-10-03-011) | Only formats are imported; no external plugin runtime is adopted. | The rejection. |
| MOD-DESK-001 (PROVISIONAL): Electron plus React and TypeScript native shell for v1 | **RETAINED, SCOPE CLARIFIED** (DR-PX-2026-10-03-012) | Main-process platform integration is specified; Electron is not replaced. SurfaceProtocol keeps architectural lock-in away. | The decision. |
| PX-049 invariant: webPreferences, CSP and preload byte-identical | **RETAINED** (DR-PX-2026-10-03-012) | PX-094 keeps the same invariant; PX-095 and PX-096 add main-process code (menus, protocol handler, fuses, session denials) under their own gates and pinned inventories. | The renderer isolation model. |
