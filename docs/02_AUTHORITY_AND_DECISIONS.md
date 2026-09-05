# Authority, Decision Register, and Conflict Resolution

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Current authority

This dossier supersedes older Modbit plans wherever those plans assume Code-OSS as the primary shell, an IDE-first product, or a separate Modbit Lite. Older v5-era PRDs and scope locks are historical provenance only.

## Decision register

| ID | Decision | Status | Implementation consequence |
|---|---|---|---|
| MOD-PROD-001 | Single Modbit product: agent-first Work + Code workspace | **LOCKED** | No Modbit Lite branch or duplicated product runtime |
| MOD-SURF-001 | No Code-OSS foundation | **LOCKED** | No VS Code workbench/extension host dependency in product path |
| MOD-SURF-002 | No built-in full IDE / Monaco architecture | **LOCKED** | Code shown through revision-bound trusted review surfaces; editing is not the primary UI |
| MOD-CORE-001 | One Modbit-owned canonical runtime | **LOCKED** | Same domain/event contracts for desktop local tasks and remote cloud tasks |
| MOD-STATE-001 | Memory is not recovery | **LOCKED** | Seven separated durability layers; no transcript reconstruction as resume mechanism |
| MOD-STATE-002 | compaction epochs + protocol persistence | **LOCKED** | Versioned compaction, stale result rejection, fork/revert cancellation, bounded sync fallback |
| MOD-STATE-003 | checkpoint epochs + delta recovery | **LOCKED** | Monotonic fencing generation, baseline+delta checkpoints, cursor replay and full rehydrate fallback |
| MOD-TOOL-001 | Dynamic task-scoped tool projection | **LOCKED** | Model sees only capabilities required by current step/task |
| MOD-TOOL-002 | Procedural Tool Runtime | **LOCKED** | Minimal model-visible composition interface backed by governed `tools.*` bindings |
| MOD-EXEC-001 | Command failure is not turn failure | **LOCKED** | Exit status becomes typed tool result and can feed repair loops |
| MOD-EXEC-002 | Durable terminal + replay window | **LOCKED** | Terminal broker survives UI detachment; offset/cursor replay required |
| MOD-AGENT-001 | Transactional subagent admission + capacity tickets | **LOCKED** | No worker starts until capacity, worktree/write-set and capability admission commits atomically |
| MOD-EFFECT-001 | Protected-effect receipt chain | **LOCKED** | High-risk writes/external effects produce hash-linked immutable receipts |
| MOD-BROWSE-001 | Live embedded browser as platform primitive | **LOCKED** | Structural CDP/AX/network control; screenshot only fallback; user observe/takeover in same session |
| MOD-SBX-001 | Hardened MicroVM-class cloud sandbox substrate | **LOCKED** | Sandbox policy remains Modbit-owned; no secrets in guest image or static env |
| MOD-CTX-001 | Retrieval-before-edit and evidence-backed Context Packs | **LOCKED** | Exact/BM25/vector/AST/graph/Git/diagnostics/runtime evidence; never vector-only |
| MOD-CTX-002 | Prompt-cache-aware compaction | **LOCKED** | Stable prompt prefix and cache key accounting are part of context economics |
| MOD-UX-001 | Attention-first fleet UX | **LOCKED** | Needs Attention / Ready for Review / Running / Waiting / Completed / Failed are first-class views |
| MOD-VERIFY-001 | Real-system evidence required for completion | **LOCKED** | Status ladder ends at COMPLETE only after release-gate E2E proof |
| MOD-DESK-001 | Electron + React/TypeScript native shell for v1 | **PROVISIONAL** | Chosen for mature Chromium embedding and desktop integration; SurfaceProtocol prevents architectural lock-in |
| MOD-EMB-001 | Local non-generative embedding model for semantic retrieval | **PROVISIONAL** | Embedding is infrastructure, not reasoning; exact/BM25/AST remains correctness fallback |
| MOD-AUTH-001 | OIDC-compatible hosted identity for cloud account plane | **PROVISIONAL** | Desktop uses authorization-code + PKCE; enterprise SSO can attach behind same identity interface |
| MOD-MOBILE-001 | Mobile/simulator control | **DEFERRED** | Architecture keeps tool namespace available; no P0 implementation |
| MOD-AUTO-001 | Broad consumer automations/scheduling | **DEFERRED** | Only task queue/remote continuation in P0; recurring automation product comes later |
| MOD-IDE-001 | Code-OSS / VS Code workbench as fallback product surface | **REJECTED** | May exist only in a separate future adapter repo, never as a dependency of Core |
| MOD-IDE-002 | Build a new general-purpose code editor | **REJECTED** | Not strategic; review/inspection surface only |
| MOD-ORCH-001 | Hidden fixed planner→builder→reviewer hierarchy copied from external references | **REJECTED** | Orchestration is explicit, typed and task-dependent; no unsupported claims about external reference internals |
| MOD-CTX-003 | Vector retrieval as sole context mechanism | **REJECTED** | Hybrid and structural retrieval required |
| MOD-CLOUD-001 | Cloud-only brain | **REJECTED** | Local Core is first-class; remote Core uses same domain/runtime contracts |

## Conflicts with older material

### Code-OSS
Older Modbit dossiers treated Code-OSS as the desktop substrate. The latest decision removes IDE scope and Code-OSS entirely from the product foundation. **Resolution: replace.** Retain only mechanism knowledge—workspace lifecycle, Git/worktrees, diagnostics/LSP, diff UX, secure IPC patterns—implemented independently.

### Monaco / editor buffer ownership
Some intermediate plans introduced a native React workspace with Monaco. Earlier and later agent-first decisions remove the full editor architecture. **Resolution: retain the no-IDE direction.** Filesystem + Git revision are canonical; code review is revision-bound and read-oriented.

### Modbit Lite
Older cross-product documents still name Modbit Lite. Latest product strategy consolidates it into Modbit. **Resolution: retire.** Shared state/sandbox lessons survive as Modbit architecture.

### External reference parity
External reference behavior is used as evidence for mechanisms only. No proprietary service, binary or implementation becomes an architectural dependency. **Resolution: retain mechanism-level inspiration; reject dependency or cloning.**

## Change control

Any future change to a **LOCKED** item requires a Decision Record containing: trigger/evidence, current behavior, proposed replacement, migration, compatibility, security impact, test impact, rollback and explicit user approval. A PR that silently changes a locked invariant fails architecture CI.


## V2 decisions added after source reconciliation

| ID | Decision | Status | Consequence |
|---|---|---|---|
| MOD-COV-001 | Mechanism-level requirement coverage is a build authority gate | **LOCKED** | No accepted requirement may disappear through summarization |
| MOD-MEDIA-001 | Typed `MediaEnvelope` and provider-neutral multimodal tool results | **LOCKED** | Images/PDF/audio/video are first-class artifacts/results with budgets and provenance |
| MOD-MEDIA-002 | Bounded PDF text→vision fallback with explicit lossy/untrusted label | **LOCKED** | No silent full-document claim or unbounded rasterization |
| MOD-INPUT-001 | Typed `STEER / COLLECT / FOLLOW_UP` input dispatch | **LOCKED** | Concurrency semantics live in Core, not clients |
| MOD-SKILL-001 | evolution-lab-style trace/wiki/skill separation inside Skill Evolution Lab | **EXPERIMENT** | No second general memory; no production self-modification; status label normalized from the compound "PROVISIONAL / EXPERIMENT-GATED" by DR-GOV-2026-09-05 with unchanged meaning |
| MOD-SKILL-002 | Skill promotion requires real qualification + rollback | **LOCKED** | Active skill head changes atomically only after gates pass |
| MOD-MM-001 | Multimodal/media requirements extend existing Modbit owners; no second runtime | **LOCKED** | Multimodal/subagent/daemon lessons map to existing owners |
| MOD-TOOL-003 | External-tool compatibility is capability parity, not name parity | **LOCKED** | Exact private tool names are never invented; canonical Modbit intent/effect schemas prevail |
| MOD-JIT-001 | Task-conditioned adaptive harness/profile generation stays shadow/eval | **EXPERIMENT** | Cannot replace Core or control production before measured gates |

## Execution policy decisions adopted 2026-09-05

Approved by DR-EPR-2026-09-05 in `05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md`. These extend existing Core/Gateway/policy/eval ownership. The historical frontier-only reasoning premise is **SUPERSEDED BY ADR-R-039..048** and frontier-token-only efficiency is **SUPERSEDED BY ADR-R-045**; those historical rows are not present as IDs in this edition. Product qualification remains required. Active specification: docs 27/38; tasks and gates: docs 49/61.

| ID | Decision | Status | Implementation consequence |
|---|---|---|---|
| ADR-R-039 | Provider-neutral routing binds eligible models/Skills in one ConditionalExecutionPlan; DIRECT/CASCADE/CRITIQUE classify executed paths only. No fixed model name is normative. | **LOCKED** | Template clause superseded by ADR-R-049/054; provider neutrality survives |
| ADR-R-040 | The Request Profiler predicts task/capability requirements and floor-success probability independent of model price, availability, cache residency and organization policy. | **LOCKED** | Intrinsic task features exclude catalog/economics/policy inputs |
| ADR-R-041 | The conditional compiler is deterministic over versioned intrinsic profile, policy, model/Skill registries, Outcome Statistics, cache/session, economics/health and budgets. | **LOCKED** | Refined by ADR-R-050/051: confidence-feasible then cheapest; full input replay |
| ADR-R-042 | All initial, escalation and independent-review/revision legs execute through the canonical Agent Runtime. | **LOCKED** | Refined by ADR-R-049/054; one executor, prevalidated slots only |
| ADR-R-043 | Independent review follows the Isolated Non-Committing Reviewer contract: canonical tracked state read-only, bounded disposable execution and ephemeral scratch writes, no canonical/persistent/external effects. | **LOCKED** | Absolute effect-less/read-only tool clause superseded by ADR-R-053 |
| ADR-R-044 | Routing and workflow changes are evaluation-gated. Production promotion requires holdout evidence and, when production traffic exists, shadow/canary/A-B evidence or an explicitly approved equivalent. | **LOCKED** | Named holdout and controlled rollout; previous-good rollback |
| ADR-R-045 | The primary economic metric is total inference cost plus human intervention and wall-clock per verified successful task, subject to a quality floor. | **LOCKED** | Quality floor constrains complete task economics; token detail remains |
| ADR-R-046 | Risk remains policy-owned: PolicyEnvelope minima and factual RealizedRisk determine required assurance; separate Acceptance Gate evaluates evidence sufficiency. | **LOCKED** | Refined by ADR-R-052; no learned weakening or risk/correctness conflation |
| ADR-R-047 | Multi-turn routing is cache-aware; a confidence-feasible alternative must remain better after lost cache, refill/cache write, switching latency and hysteresis. | **LOCKED** | Soft utility clause superseded by ADR-R-050; valid boundaries do not grant runtime branch synthesis |
| ADR-R-048 | Every compound request is completely accounted: all solver, critic, revision, escalation, retry and fallback legs are included in cost and telemetry. | **LOCKED** | Include failed/cancelled/retried legs and uncertain charges |

## EPR v1.1 amendments and new decisions

Approved by DR-EPR-2026-09-05-v1.1 in `06_EPR_V1_1_SUPERSESSION_DECISION.md`. Existing ADR-R-039/041/042/043/046/047 rows above carry current amended language; their conflicting original clauses are retained as immutable v1.0 source provenance. No ID collision occurred. Non-conflicting decisions remain locked.

| ID | Decision | Status | Implementation consequence |
|---|---|---|---|
| ADR-R-049 | DIRECT, CASCADE and CRITIQUE are execution-path classifications, not separate orchestration engines. The canonical runtime object is a bounded conditional transaction with prevalidated continuation slots. | **LOCKED** | One conditional transaction; derived path labels |
| ADR-R-050 | Plan eligibility uses a confidence-adjusted verified-quality floor. Cheap plans are not eligible solely because their point-estimate quality exceeds the floor. | **LOCKED** | Conservative feasibility precedes cost ranking; explicit infeasible fallback |
| ADR-R-051 | Empirical solver success, escalation probability, reviewer value and revision success live in a separately versioned Outcome Statistics Store rather than the Model Registry. | **LOCKED** | Existing eval-bench owns separate versioned derived data, no duplicate state authority |
| ADR-R-052 | Realized risk determines required assurance; the Acceptance Gate determines whether available evidence satisfies that assurance. Risk and correctness are not one classifier. | **LOCKED** | Factual assurance versus evidence sufficiency |
| ADR-R-053 | Independent review executes as an Isolated Non-Committing Reviewer: read-only on canonical tracked state, ephemeral writes and bounded execution only in a throwaway review environment, no persistent or external effects. | **LOCKED** | Disposable bounded review execution; canonical/external effects denied |
| ADR-R-054 | Runtime escalation/review/human branches may activate only precompiled, prevalidated continuation slots whose worst-case budget and policy eligibility were checked before execution. | **LOCKED** | All slots validated/reserved before initial dispatch |
| ADR-R-055 | Request, leg and gate outcomes are logged separately so successful escalation does not misattribute success to a failed earlier leg. | **LOCKED** | Exact request/leg/gate attribution with versioned provenance |
| ADR-R-056 | Gate calibration is release-critical and evaluated independently of router quality. Router improvements cannot excuse an unacceptable false-accept or critical-risk-miss rate. | **LOCKED** | Independent gate/risk/critical-surface thresholds block unsafe rollout |

| Superseding decision | Earlier decision | Scope |
|---|---|---|
| `ADR-R-049` | `ADR-R-039` | Primary template selection replaced; model neutrality retained |
| `ADR-R-049` | `ADR-R-042` | Conditional slots refine shared executor |
| `ADR-R-050` | `ADR-R-041` | Confidence-feasibility and cost ordering refine deterministic compiler |
| `ADR-R-050` | `ADR-R-047` | Feasible switch economics replace soft utility |
| `ADR-R-051` | `ADR-R-041` | Separately versioned statistics added to compiler inputs |
| `ADR-R-052` | `ADR-R-046` | Required assurance separated from evidence acceptance |
| `ADR-R-053` | `ADR-R-043` | Disposable review writes/exec replace absolute effect-less tool restriction |
| `ADR-R-054` | `ADR-R-042` | Continuation graph prevalidated before transaction |
| `ADR-R-055` | `ADR-R-048` | Request/leg/gate attribution refines complete accounting |
| `ADR-R-056` | `ADR-R-044` | Independent gate/risk calibration strengthens promotion |

## Governance maintenance record (DR-GOV-2026-09-05)

Approved in `96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md`. It changes no requirement, owner, ADR or EPR clause. It normalizes the MOD-SKILL-001 status label to the single value `EXPERIMENT` (the decision was already experiment-gated; its consequence is unchanged), defines the evidence-reference grammar in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`, aligns `AGENTS.md`, `README.md` and `SKILLS.md` with the sealed package and reseal sequence, and records the pinned tool constants in `74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md`. Decision statuses are now validated against the doc 93 ladder by `tools/build_graph.py` and `tools/check_dossier.py`.
