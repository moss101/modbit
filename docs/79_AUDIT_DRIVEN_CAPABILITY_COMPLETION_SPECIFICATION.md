# Audit-driven capability completion specification

> **Authority:** DR-PX-2026-10-05-013 (`decisions/DR-PX-2026-10-05-013-audit-driven-capability-completion.md`), status **accepted** on 2026-10-05 (owner instruction, goal: implement research/audit/01-TASK-LIST.md). Rows PX-099..PX-139 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED; each lists DOC-PX-013 as a prerequisite, so none may start before DOC-PX-013 is COMPLETE, and each is startable as its other prerequisites complete.  
> **Nature:** a specification of the build tasks of the 2026-10-05 audit that no earlier PX row fully covers. It states, for every task of that list, how it maps to the ledger, what exists, what is the first missing link and what proves it done. It is not an implementation and not proof that anything exists.

## 1. Purpose

An audit of `main` at 5fdb47f1 (nine read-only parts, A to I, in the untracked `research/audit` folder) found the Core real and deep and the layer between the Core and the user thin, and produced a task list: eight verification tasks (VER), twenty-one fixes (FIX), four dossier tasks (DOC) and thirty build tasks (BLD). The fixes are done on an unmerged branch (pull request moss101/modbit#61). The owner asked for the end-to-end implementation of the list. A build task cannot start without a ledger row, a card, a qualification and a scenario (doc 46), and three accepted records (DR-PX-2026-10-03-007, -008 and -010) already supply rows for part of it. This document and DR-PX-2026-10-05-013 supply the rest: 41 rows over existing owners, with every BLD task traced to rows in section 7.

## 2. Basis and evidence key

The parity specifications of docs 65 to 69 tag facts LIVE, DISK, STATIC and UNVERIFIED because they derive from observing another product. These rows derive from an audit of Modbit itself, so the key is different and says how well each finding is known. It never lowers a proof: a row is closed only by a test against the real boundary it names.

| Basis | Meaning |
|---|---|
| STATIC-AUDIT | Read in the code by the audit at 5fdb47f1 with file and line evidence and not executed. The first step of each implementing task is an existing-code audit on the merged state, because the audit fixes moved the code. |
| REPRODUCED | Reproduced by Phase 0 (`02-PHASE0-AND-FIX-RESULTS.md`) on the integrated audit-fix branch. |
| FIX-DELIVERED | Delivered by an audit fix on the unmerged branch. A row that builds on one names it and its implementing task confirms the merged state before it starts. |

Every requirement below uses STATIC-AUDIT for its own behaviour; REPRODUCED and FIX-DELIVERED appear in the audit table of section 4 for what was proven or already built.

## 3. Constraints and ownership

- **No new subsystem.** Every row maps to an existing owner of doc 81 and the graph; where a row needs a module it is registered under that owner (the pattern of DR-M9-001). One orchestration graph, one event store, one protocol state, one policy kernel, one approval and effect ledger, one checkpoint engine, one terminal broker, one workspace and change engine, one provider gateway: nothing here adds a second.
- **EPR-pinned behaviour is untouched.** No EPR-000..019 row, gate, threshold, ADR-R clause or algorithm changes. PX-133 conforms the Outcome Statistics emission to ADR-R-051 and ADR-R-055 as sealed, PX-134 adds operator verbs for the signed registry of EPR-002, PX-135 widens calibration data for EPR-019 and PX-136 measures and attests nothing. Section 9 lists what is deliberately not adopted because it would change pinned behaviour.
- **Unratified records are not depended on.** DR-PX-2026-10-03-009 (Workspace Editor), -011 (marketplace and Customize) and -012 (shell platform integration) are proposed. No row here depends on PX-077..098 or on DOC-PX-009, DOC-PX-011 or DOC-PX-012. PX-106 is a Customize view limited to the skills, hooks and extensions that exist today (no catalog, no install from a network location); PX-117 extends the hook events without PX-092. If -011 is accepted later, PX-093 extends the PX-106 view and must not create a second Customize surface. A test asserts the separation.
- **Real-effect proof and failure injection in every row.** Each qualification names the real process, filesystem, Git repository, Chromium, provider, SQLite or staging service it crosses and the fault it injects, and a mutation that must make it fail (doc 55).
- **Owner-supplied inputs are named, not assumed.** See section 10.
- **Evidence tiers by behavioural risk** (doc 83): 39 rows release-critical and 2 iteration (PX-137 and PX-138, measurement only). The task list tagged BLD-30 iteration; PX-139 is release-critical because the child-cost rollup extends the accounting records EPR-010 reads.

### Owner map of the new rows

| Owner | Rows |
|---|---|
| terminal | PX-099, PX-132 |
| desktop | PX-100, PX-104, PX-106, PX-127, PX-128 |
| core-runtime | PX-101, PX-116 |
| durability | PX-102, PX-109 |
| workspace-git | PX-103, PX-118, PX-119 |
| skills | PX-105, PX-107 |
| context-engine | PX-108, PX-110, PX-111, PX-112 |
| memory | PX-113 |
| procedural-runtime | PX-114 |
| external-tools | PX-115, PX-125 |
| extensions-hooks | PX-117 |
| browser | PX-120, PX-121, PX-122, PX-123, PX-124 |
| sandbox-cloud | PX-126, PX-129 |
| effects-security | PX-130, PX-131 |
| eval-bench | PX-133, PX-135, PX-136, PX-137, PX-138 |
| model-gateway | PX-134 |
| observability | PX-139 |

## 4. Existing implementation audit

Classes of doc 93 at `main` 5fdb47f1, from the audit summary, with what the audit fixes (FIX-01..21, unmerged) delivered since. The existing rows are those of doc 62 that already carry part of the task.

| BLD | Task | Audit areas | Classification at the baseline | Delivered by audit fixes | First missing link | Coverage |
|---|---|---|---|---|---|---|
| BLD-01 | Conversation projection and streaming | 1, 16 | NOT-FOUND in the client; Core data PRODUCTION-WORKING | FIX-21 (linear reducer, Core attention items) | an ephemeral delta frame, a transcript projection and a renderer that shows them | Covered by existing rows |
| BLD-02 | Terminal stream and view | 33 to 35 | IMPLEMENTED-PARTIAL (execd PRODUCTION-WORKING, no stream to clients, no ACK or resize) | FIX-09 (one object store), FIX-20 (bounded window, on-disk index, ownership) | the ACK and resize frames and the agent tools; the client stream and view are PX-043 and PX-048 | Principal part covered; residual is new |
| BLD-03 | Modes and execution preference | 36, 37 | IMPLEMENTED-PARTIAL (`SetExecutionPreference` DOCUMENTED-ONLY) | none | client verbs; the Core commands are PX-051 and PX-053 | Principal part covered; residual is new |
| BLD-04 | Pause, resume, per-turn checkpoints, fork from turn | 2, 15 | IMPLEMENTED-PARTIAL (pause and resume NOT-FOUND; checkpoints PRODUCTION-WORKING without names or collection) | FIX-17 (idle exit), FIX-18 (HEAD drift refusal) | PauseTask and ResumeTask; label and collector | Principal part covered; residual is new |
| BLD-05 | Review and changes panel | 17 | IMPLEMENTED-PARTIAL (`change.*` PRODUCTION-WORKING; shell and test writes were outside the barrier) | FIX-06 (bounded snapshot diff attributed to the tool call) | attribution on hunks, decisions during a run, Review in any state | Principal part covered; residual is new |
| BLD-06 | Design foundation | 1 | SCAFFOLDED (`packages/ui` and `packages/design-tokens` are stubs) | none | tokens, primitives, gallery and contrast gate | Covered by existing rows |
| BLD-07 | Skills made usable | 20, 21, 55 | IMPLEMENTED-PARTIAL and SCAFFOLDED (skills inert for normal users) | none | trust command, System scope, index, skill.load, Customize over existing registries | Principal part covered; residual is new |
| BLD-08 | AGENTS.md and CLAUDE.md auto-load, pre-turn pack | 7, 8 | NOT-FOUND | FIX-04 (rules trust-gated), FIX-12 (stale fragments) | a native rules layer and a pre-turn planner call | Not covered; new rows |
| BLD-09 | Structured compaction summary | 14 | IMPLEMENTED-PARTIAL (chain PRODUCTION-WORKING; summary extractive) | FIX-07 (aligned cut, Core-event facts), FIX-13 (stable prompt prefix) | a registry-role summarizer with validation and fallback | Not covered; new rows |
| BLD-10 | Code graph | 11 | IMPLEMENTED-PARTIAL (file-level only) | FIX-12 (retrieval quality) | identifier occurrence resolution and non-test impact | Not covered; new rows |
| BLD-11 | Persisted incremental indexes, embedder, indexed grep | 7, 8 | IMPLEMENTED-PARTIAL | FIX-12 | an on-disk index store; an admitted embedding model | Not covered; new rows |
| BLD-12 | Memory wired | 13 | IMPLEMENTED-PARTIAL (never injected; mutations bypass the log) | none | a pack segment and event-sourced mutations | Not covered; new rows |
| BLD-13 | Exec-only projection mode | 22, 23 | IMPLEMENTED-PARTIAL (mechanism PRODUCTION-WORKING; about 28 to 30 schemas per request) | none | a projection mode, a schema budget and a live trial | Not covered; new rows |
| BLD-14 | MCP lazy discovery | 24 | IMPLEMENTED-PARTIAL (eager schemas, no list_changed) | none | describe on demand, reaper, large-catalog benchmark | Not covered; new rows |
| BLD-15 | Hierarchical budgets | 38 | IMPLEMENTED-PARTIAL (turns and tool calls only) | FIX-16 (cancellation cascade) | cost and clock fields and reservation at admission | Not covered; new rows |
| BLD-16 | Hook points and context injection | 51 | PRODUCTION-WORKING for 12 events, command-only | none | new events and a labelled context field | Not covered; new rows |
| BLD-17 | Worktree isolation as a task option and lifecycle | 18 | IMPLEMENTED-PARTIAL (library capability; tasks write in the checkout) | FIX-01 (confined worktree paths) | an isolation field that binds the run's root; the lifecycle is PX-065 | Principal part covered; residual is new |
| BLD-18 | Merge integration | 18, 19 | SCAFFOLDED (merge transaction has no production caller) | FIX-16 | git.merge tools, persisted state, post-merge verification, completion gate; apply-back is PX-066 | Principal part covered; residual is new |
| BLD-19 | Browser hardening | 30 | IMPLEMENTED-PARTIAL | FIX-19 (partial; Electron e2e written, not run) | target policy, feedback primitives, deltas; the page-origin gate and CDP deny list are PX-073 | Principal part covered; residual is new |
| BLD-20 | Semantic compiler | 31 | IMPLEMENTED-PARTIAL (deltas pull-based; no classification or derived actions) | none | classification, forms, derived actions, risk classification | Not covered; new rows |
| BLD-21 | WebMCP rung | 30 | NOT-FOUND | none | discovery of page-declared tools in the host | Not covered; new rows |
| BLD-22 | Native computer control | 32 | NOT-FOUND (record accepted 2026-10-05) | none | implementation of PX-069 to PX-076 | Covered by existing rows |
| BLD-23 | GitHub-native completion | 57, 58 | IMPLEMENTED-PARTIAL (proven on a fake; ingestion has no caller) | none | PR read and diff, check and comment webhooks, rendering, the first real-GitHub run | Not covered; new rows |
| BLD-24 | Cloud client | 1, 44 | NOT-FOUND in the clients (server side real) | none | client verbs; OIDC and PKCE; provisioning; signed policy-bundle route | Not covered; new rows |
| BLD-25 | Automation layer | 28, 29 | NOT-FOUND (record accepted 2026-10-05) | none | implementation of PX-082 to PX-086 | Covered by existing rows |
| BLD-26 | Credential broker unification, AuthorizationEpoch and CapabilitySnapshot | 39 to 43 | IMPLEMENTED-PARTIAL (broker fragmented); DOCUMENTED-ONLY (snapshot and epoch) | FIX-08 (dispatch receipt, atomic chain) | one broker interface; the snapshot and epoch records | Not covered; new rows |
| BLD-27 | Process intelligence | 53 | NOT-FOUND | none | listening-socket discovery per process tree and readiness | Not covered; new rows |
| BLD-28 | EPR reachability and evidence | 37 | IMPLEMENTED-PARTIAL (code real; unreachable by users; Solver samples only) | none | Escalation and Reviewer samples, operator verbs, corpora, a live paired run | Not covered; new rows |
| BLD-29 | Benchmarks | 60 | IMPLEMENTED-PARTIAL (eight hand-seeded cases, no external baseline) | FIX-12 (Recall@5 improved on the existing corpus) | an external baseline profile, fifty cases, live same-model measurement | Not covered; new rows |
| BLD-30 | Observability | 59 | PRODUCTION-WORKING without OpenTelemetry or child-cost rollup | none | an exporter over the accounting records, health persistence | Not covered; new rows |

Four tasks are fully covered by existing rows, eight are covered in their principal part and keep a residual that is a new row, and eighteen are not covered at all.

## 5. Requirements

Each requirement is the invariant a row must establish; the basis says how it is known. Numbering is by group.

### A. Reachability of the Core (clients, review, skills)

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-A01 | The terminal broker serves attach-from-cursor with acknowledgement and resize frames and never holds unbounded output for a slow client. | STATIC-AUDIT | PX-099 |
| ADC-A02 | An agent reaches its own shells only through typed shell.input and shell.attach tools under the Capability Kernel. | STATIC-AUDIT | PX-099 |
| ADC-A03 | The CLI and the IDE adapter can set the task mode and the execution preference through the same typed commands as the desktop, and show the Core's answer. | STATIC-AUDIT | PX-100 |
| ADC-A04 | A person can pause a running local task and resume it later, across a Core restart, through typed commands that use the existing park mechanism. | STATIC-AUDIT | PX-101 |
| ADC-A05 | Checkpoints can be named and are retained and collected by a stated policy that never deletes a checkpoint a live task or fork still needs. | STATIC-AUDIT | PX-102 |
| ADC-A06 | Every hunk of a change names the tool call that produced it, including writes made by shell and test tools. | STATIC-AUDIT | PX-103 |
| ADC-A07 | The review surface is reachable while a task runs, waits, fails or completes, and every hunk shows the tool call that produced it. | STATIC-AUDIT | PX-104 |
| ADC-A08 | A user-authored skill becomes active only through a content-hash trust decision the user can make from the CLI, and reaches the model only through a bounded index and an explicit load. | STATIC-AUDIT | PX-105 |
| ADC-A09 | A desktop view lists and manages the skills, hooks and installed extensions that exist today, with their scope, trust and provenance, without any catalog or install-from-network feature. | STATIC-AUDIT | PX-106 |

### B. Instructions, context and memory

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-B01 | A repository's AGENTS.md and CLAUDE.md reach the model as a native rules layer only in a trusted repository, with a stated precedence and a size cap. | STATIC-AUDIT | PX-107 |
| ADC-B02 | A task starts with a retrieval pack seeded from its goal, not only when the model asks for one, with the same revision and provenance rules. | STATIC-AUDIT | PX-108 |
| ADC-B03 | A model-written structured summary replaces the extractive one when it validates, and the extractive path remains the fail-closed fallback. | STATIC-AUDIT | PX-109 |
| ADC-B04 | The evidence graph holds reference, call and implementation edges, and impact analysis returns impacted non-test files with the evidence for each edge. | STATIC-AUDIT | PX-110 |
| ADC-B05 | Indexes survive a Core restart and refresh incrementally, and exact search uses an index instead of a full scan. | STATIC-AUDIT | PX-111 |
| ADC-B06 | Semantic retrieval can use a learned local embedding model and an optional reranker, admitted through the dependency policy and measured against the hybrid baseline. | STATIC-AUDIT | PX-112 |
| ADC-B07 | Promoted memories reach later sessions' prompts through the pack compiler with provenance, and every promote, edit and forget is an event. | STATIC-AUDIT | PX-113 |

### C. Tool surface, budgets and hooks

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-C01 | A projection mode exposes the small procedural surface, defers agent.* and repair tools until delegating, and is bounded by a schema-bytes budget recorded per request. | STATIC-AUDIT | PX-114 |
| ADC-C02 | External tool schemas are discovered on demand and the catalog stays current, within a schema budget, with idle servers reaped. | STATIC-AUDIT | PX-115 |
| ADC-C03 | A child's budget is clamped to and reserved against its parent's remainder across turns, tool calls, cost and wall clock, and its spend rolls up. | STATIC-AUDIT | PX-116 |
| ADC-C04 | Hooks cover more lifecycle events and may add labelled context that the Kernel can refuse, and can never widen authority. | STATIC-AUDIT | PX-117 |

### D. Worktrees and merge integration

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-D01 | A task can be created with worktree isolation so that every tool of the run operates in its own worktree and never in the user's checkout. | STATIC-AUDIT | PX-118 |
| ADC-D02 | Child branches are integrated by governed git.merge tools over the merge transaction, with persisted state, verification after the merge and a completion gate on integration. | STATIC-AUDIT | PX-119 |

### E. Browser runtime

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-E01 | The browser host refuses navigation to loopback, private, link-local and metadata targets unless the task's policy names them, including through redirects and name rebinding. | STATIC-AUDIT | PX-120 |
| ADC-E02 | A coding agent can see what its page did (viewport, console, network), scroll and wait, and a browser action whose outcome is unknown latches instead of being retried blindly. | STATIC-AUDIT | PX-121 |
| ADC-E03 | Deltas are produced by the host as the page changes, a client can ask for changes since a fingerprint, frames are covered, and element identities survive a restart. | STATIC-AUDIT | PX-122 |
| ADC-E04 | The compiler classifies the page, groups forms, derives higher-level actions with postconditions and classifies their risk from the destination and the dialog, not only the control. | STATIC-AUDIT | PX-123 |
| ADC-E05 | Tools a page declares itself are surfaced as untrusted, origin-bound proposals that the Kernel authorises like any external tool. | STATIC-AUDIT | PX-124 |

### F. GitHub-native work and the cloud client

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-F01 | The forge adapter can read a pull request and its diff and post status comments, as typed tools behind the Kernel, and has been exercised against real GitHub. | STATIC-AUDIT | PX-125 |
| ADC-F02 | Check-suite and comment events from GitHub reach the CI-result and review-comment ingestion that exist today, with the same signature, replay and tenant checks. | STATIC-AUDIT | PX-126 |
| ADC-F03 | CI evidence and review comments are visible in Review and the CLI, and one issue travels to a merged-ready pull request with its CI result on real GitHub. | STATIC-AUDIT | PX-127 |
| ADC-F04 | A person can continue a local task in the cloud, watch and approve it from the CLI and desktop, and bring it back, through first-party clients. | STATIC-AUDIT | PX-128 |
| ADC-F05 | Clients sign in to the cloud through OIDC with PKCE, tenants and principals are provisioned by API, and organisation policy bundles are published signed. | STATIC-AUDIT | PX-129 |

### G. Policy kernel and credentials

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-G01 | Every credential the product uses is resolved through one broker interface by handle, and the crate named as its owner contains it. | STATIC-AUDIT | PX-130 |
| ADC-G02 | Each model round runs against one frozen capability snapshot with an authorization epoch stamped on every decision and receipt, and a policy change takes effect at the next round boundary. | STATIC-AUDIT | PX-131 |

### H. Process intelligence

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-H01 | The Core knows which ports a task's processes listen on, whether a dev server is ready and healthy, without asking the model. | STATIC-AUDIT | PX-132 |

### I. Execution policy router evidence (conformance and measurement only)

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-I01 | Production request records yield Escalation and Reviewer samples with complete priced cost, as ADR-R-051 and ADR-R-055 already require, not only Solver samples. | STATIC-AUDIT | PX-133 |
| ADC-I02 | An operator can sign, verify and activate a model registry bundle from the CLI, with revocation and the previous-good rollback that the specification already requires. | STATIC-AUDIT | PX-134 |
| ADC-I03 | The Acceptance Gate is calibrated on held-out corpora covering the Tier A languages and the false-accept classes found so far, with oracle labels and intervals. | STATIC-AUDIT | PX-135 |
| ADC-I04 | One live experiment measures direct, cascade and critique on the same tasks with two real model bindings, with complete cost and honest intervals. | STATIC-AUDIT | PX-136 |

### J. Benchmarks and observability

| ID | Requirement | Basis | Row |
|---|---|---|---|
| ADC-J01 | Retrieval claims are measured against a real external baseline on at least fifty cases and a larger public repository, with live same-model accuracy, token and tool-call measurement. | STATIC-AUDIT | PX-137 |
| ADC-J02 | Compaction quality is evaluated by whether a run still completes its task and recalls needed facts afterwards, judged against an oracle outside the summarizer. | STATIC-AUDIT | PX-138 |
| ADC-J03 | Traces and metrics can be exported through OpenTelemetry with parent and child spans carrying cost, and component health survives a restart. | STATIC-AUDIT | PX-139 |

## 6. Rows and evidence tiers

| Row | Title | Owner | Tier | Prerequisites |
|---|---|---|---|---|
| PX-099 | Terminal flow control (ACK, resize, bounded ring) and agent-facing shell.input and shell.attach tools | terminal | release-critical | PX-043, M2.3, M4.5, DOC-PX-013 |
| PX-100 | CLI and IDE-adapter controls for task mode and execution preference | desktop | release-critical | PX-051, PX-053, PX-002, DOC-PX-013 |
| PX-101 | PauseTask and ResumeTask over the runtime park, with local persistence | core-runtime | release-critical | M2.7, M6.7, M4.4, DOC-PX-013 |
| PX-102 | Named checkpoints, retention and garbage collection | durability | release-critical | PX-061, M4.3, DOC-PX-013 |
| PX-103 | Hunk attribution to the originating tool call and per-hunk decisions that keep the run going | workspace-git | release-critical | M2.9, M4.3, PX-061, DOC-PX-013 |
| PX-104 | Review reachable from any task state, with live per-hunk controls and the originating-call chip | desktop | release-critical | PX-048, PX-103, PX-062, DOC-PX-013 |
| PX-105 | Skills a user can use: trust by command, a System scope, an index under a budget, skill.load and path gating | skills | release-critical | M5.5, PX-052, DOC-PX-013 |
| PX-106 | Customize view over the existing skills, hooks and extensions (no catalog) | desktop | release-critical | PX-044, PX-045, PX-052, PX-105, DOC-PX-013 |
| PX-107 | AGENTS.md and CLAUDE.md as a trust-gated native rules layer | skills | release-critical | IMP-EV-0129, IMP-EV-0059, DOC-PX-013 |
| PX-108 | Goal-seeded pre-turn context pack | context-engine | release-critical | M3.7, M3.8, PX-015, DOC-PX-013 |
| PX-109 | Structured compaction summary with extractive fail-closed fallback, model-aware thresholds and a searchable transcript pointer | durability | release-critical | M4.2, IMP-EV-0130, M2.6, DOC-PX-013 |
| PX-110 | Code graph: symbol reference and call edges, implementors and non-test impact selection | context-engine | release-critical | M3.3, M3.6, IMP-EV-0157, DOC-PX-013 |
| PX-111 | Persisted incremental indexes and an indexed grep path | context-engine | release-critical | M3.1, M3.2, M3.5, IMP-EV-0172, DOC-PX-013 |
| PX-112 | Learned embedder and rerank option behind dependency admission | context-engine | release-critical | M3.5, PX-111, DOC-PX-013 |
| PX-113 | Engineering memory wired into the prompt and the event log, with Agent and Space scopes | memory | release-critical | M9.1, IMP-EV-0162, IMP-EV-0129, DOC-PX-013 |
| PX-114 | Exec-only tool projection mode, a schema-bytes budget and a live paired trial | procedural-runtime | release-critical | M5.1, M5.4, M5.6, IMP-EV-0116, DOC-PX-013 |
| PX-115 | Lazy MCP discovery, list_changed, idle reaper and a large-catalog benchmark | external-tools | release-critical | M9.4, IMP-EV-0128, IMP-EV-0177, DOC-PX-013 |
| PX-116 | Hierarchical cost and wall-clock budgets, max_children and read-scope enforcement | core-runtime | release-critical | M6.2, M6.3, IMP-EV-0048, IMP-EV-0032, DOC-PX-013 |
| PX-117 | More hook points, kernel-gated hook context injection and prompt-type hooks | extensions-hooks | release-critical | IMP-EV-0139, IMP-EV-0239, IMP-EV-0042, DOC-PX-013 |
| PX-118 | Worktree isolation as a typed task option: the run executes in its own worktree | workspace-git | release-critical | PX-065, PX-067, IMP-EV-0145, DOC-PX-013 |
| PX-119 | Agent merge integration: git.merge tools, persisted merge state, post-merge verification and parent completion gated on integrated children | workspace-git | release-critical | PX-118, PX-066, M6.5, IMP-EV-0067, DOC-PX-013 |
| PX-120 | Browser navigation target policy: loopback, private ranges, link-local, metadata endpoints, redirects and rebinding | browser | release-critical | PX-073, M7.1, M7.7, DOC-PX-013 |
| PX-121 | Browser feedback primitives: viewport capture, console, network, scroll, wait, an escalation field and the unknown-outcome latch | browser | release-critical | PX-073, PX-069, M7.4, M7.5, DOC-PX-013 |
| PX-122 | Page-state deltas from a host mutation observer, since_fingerprint, multi-frame accessibility trees and stable identities across restart | browser | release-critical | M7.2, M7.3, PX-121, DOC-PX-013 |
| PX-123 | Semantic compiler: page classification, forms and fill_form, intent filter, derived actions and stronger action risk classification | browser | release-critical | PX-122, M7.4, IMP-EV-0088, DOC-PX-013 |
| PX-124 | WebMCP rung: page-declared tools with origin binding and trust labels | browser | release-critical | PX-073, PX-120, M9.4, IMP-EV-0281, DOC-PX-013 |
| PX-125 | GitHub pull request read, diff and status comments through the forge adapter | external-tools | release-critical | PX-006, PX-007, M9.4, DOC-PX-013 |
| PX-126 | Check-suite and comment webhooks into CI-result and review-comment ingestion | sandbox-cloud | release-critical | PX-011, PX-008, PX-009, PX-125, DOC-PX-013 |
| PX-127 | ci_evidence and review comments in Review and the CLI, and the first real-GitHub issue-to-task-to-PR-to-CI proof | desktop | release-critical | PX-125, PX-126, PX-048, PX-010, DOC-PX-013 |
| PX-128 | Cloud handoff, watch and approve verbs in the CLI and the desktop | desktop | release-critical | M8.7, M8.1, PX-042, PX-045, DOC-PX-013 |
| PX-129 | OIDC authorization-code with PKCE sign-in, tenant provisioning and the signed policy-bundle publisher route | sandbox-cloud | release-critical | M8.1, M8.2, DOC-PX-013 |
| PX-130 | Credential broker unification: one broker interface and handle path for tools, providers, MCP, browser and cloud | effects-security | release-critical | M8.6, M7.8, M9.3, IMP-EV-0288, DOC-PX-013 |
| PX-131 | AuthorizationEpoch and CapabilitySnapshot: the capability view frozen per model round | effects-security | release-critical | M2.5, M9.2, IMP-EV-0041, DOC-PX-013 |
| PX-132 | Process intelligence: port and dev-server detection, readiness and service health reported by the Core | terminal | release-critical | PX-043, M2.3, M4.5, DOC-PX-013 |
| PX-133 | Escalation and reviewer leg samples with priced cost in Outcome Statistics | eval-bench | release-critical | EPR-010, EPR-015, EPR-011, DOC-PX-013 |
| PX-134 | Model registry sign, verify and activate verbs on the CLI | model-gateway | release-critical | EPR-002, PX-053, DOC-PX-013 |
| PX-135 | Widened gate calibration corpora across languages with generated adversarial checks | eval-bench | release-critical | EPR-019, EPR-017, PX-028, DOC-PX-013 |
| PX-136 | One live paired benchmark of DIRECT, CASCADE and CRITIQUE on first-party providers | eval-bench | release-critical | PX-133, PX-134, PX-053, EPR-012, DOC-PX-013 |
| PX-137 | Retrieval benchmark with an external baseline profile, fifty or more cases, a larger public repository and live same-model measurement | eval-bench | iteration | M3.9, IMP-EV-0250, IMP-EV-0251, DOC-PX-013 |
| PX-138 | Compaction evaluation that is not self-referential | eval-bench | iteration | M4.2, PX-109, IMP-EV-0274, DOC-PX-013 |
| PX-139 | OpenTelemetry export, child-cost rollup and persisted health | observability | release-critical | M10.1, PX-116, IMP-EV-0032, DOC-PX-013 |

## 7. Traceability: BLD task to rows, owner, tier and prerequisites

Existing rows are those of doc 62 accepted before this record (their tier is that declared in their records: PX-044 to PX-047, PX-060 and PX-064 iteration, every other release-critical). The prerequisites column lists the graph prerequisites of the new rows other than DOC-PX-013, which every new row carries. Audit fix tasks (FIX-nn) are not graph nodes and cannot gate a row: a row that builds on one says so, and its implementing task confirms the merged state first.

| BLD | Existing rows | New rows | Owner | Tier | Prerequisites of the new rows | Coverage |
|---|---|---|---|---|---|---|
| BLD-01 | PX-041, PX-042, PX-047 | none | domain-events, desktop | release-critical (PX-041, PX-042) and iteration (PX-047) | see doc 62 | full by existing rows |
| BLD-02 | PX-043, PX-048 | PX-099 | terminal, desktop | release-critical | PX-043, M2.3, M4.5 | partial: residual in new rows |
| BLD-03 | PX-051, PX-053, PX-054, PX-056 | PX-100 | core-runtime, model-gateway, desktop | release-critical | PX-051, PX-053, PX-002 | partial: residual in new rows |
| BLD-04 | PX-061, PX-062 | PX-101, PX-102 | durability, desktop, core-runtime | release-critical | M2.7, M6.7, M4.4, PX-061, M4.3 | partial: residual in new rows |
| BLD-05 | PX-048, PX-061, PX-062 | PX-103, PX-104 | desktop, durability, workspace-git | release-critical | M2.9, M4.3, PX-061, PX-048, PX-103, PX-062 | partial: residual in new rows |
| BLD-06 | PX-044 | none | desktop | iteration | see doc 62 | full by existing rows |
| BLD-07 | PX-052 | PX-105, PX-106 | skills, desktop | release-critical | M5.5, PX-052, PX-044, PX-045, PX-105 | partial: residual in new rows |
| BLD-08 | none | PX-107, PX-108 | skills, context-engine | release-critical | IMP-EV-0129, IMP-EV-0059, M3.7, M3.8, PX-015 | new rows only |
| BLD-09 | none | PX-109 | durability | release-critical | M4.2, IMP-EV-0130, M2.6 | new rows only |
| BLD-10 | none | PX-110 | context-engine | release-critical | M3.3, M3.6, IMP-EV-0157 | new rows only |
| BLD-11 | none | PX-111, PX-112 | context-engine | release-critical | M3.1, M3.2, M3.5, IMP-EV-0172, PX-111 | new rows only |
| BLD-12 | none | PX-113 | memory | release-critical | M9.1, IMP-EV-0162, IMP-EV-0129 | new rows only |
| BLD-13 | none | PX-114 | procedural-runtime | release-critical | M5.1, M5.4, M5.6, IMP-EV-0116 | new rows only |
| BLD-14 | none | PX-115 | external-tools | release-critical | M9.4, IMP-EV-0128, IMP-EV-0177 | new rows only |
| BLD-15 | none | PX-116 | core-runtime | release-critical | M6.2, M6.3, IMP-EV-0048, IMP-EV-0032 | new rows only |
| BLD-16 | none | PX-117 | extensions-hooks | release-critical | IMP-EV-0139, IMP-EV-0239, IMP-EV-0042 | new rows only |
| BLD-17 | PX-065 | PX-118 | workspace-git | release-critical | PX-065, PX-067, IMP-EV-0145 | partial: residual in new rows |
| BLD-18 | PX-066 | PX-119 | workspace-git | release-critical | PX-118, PX-066, M6.5, IMP-EV-0067 | partial: residual in new rows |
| BLD-19 | PX-073 | PX-120, PX-121, PX-122 | browser | release-critical | PX-073, M7.1, M7.7, PX-069, M7.4, M7.5, M7.2, M7.3, PX-121 | partial: residual in new rows |
| BLD-20 | none | PX-123 | browser | release-critical | PX-122, M7.4, IMP-EV-0088 | new rows only |
| BLD-21 | none | PX-124 | browser | release-critical | PX-073, PX-120, M9.4, IMP-EV-0281 | new rows only |
| BLD-22 | PX-069, PX-070, PX-071, PX-072, PX-073, PX-074, PX-075, PX-076 | none | browser, effects-security, desktop, core-runtime, media | release-critical | see doc 62 | full by existing rows |
| BLD-23 | none | PX-125, PX-126, PX-127 | external-tools, sandbox-cloud, desktop | release-critical | PX-006, PX-007, M9.4, PX-011, PX-008, PX-009, PX-125, PX-126, PX-048, PX-010 | new rows only |
| BLD-24 | none | PX-128, PX-129 | desktop, sandbox-cloud | release-critical | M8.7, M8.1, PX-042, PX-045, M8.2 | new rows only |
| BLD-25 | PX-082, PX-083, PX-084, PX-085, PX-086 | none | automation, core-runtime, effects-security, sandbox-cloud, desktop | release-critical | see doc 62 | full by existing rows |
| BLD-26 | none | PX-130, PX-131 | effects-security | release-critical | M8.6, M7.8, M9.3, IMP-EV-0288, M2.5, M9.2, IMP-EV-0041 | new rows only |
| BLD-27 | none | PX-132 | terminal | release-critical | PX-043, M2.3, M4.5 | new rows only |
| BLD-28 | none | PX-133, PX-134, PX-135, PX-136 | eval-bench, model-gateway | release-critical | EPR-010, EPR-015, EPR-011, EPR-002, PX-053, EPR-019, EPR-017, PX-028, PX-133, PX-134, EPR-012 | new rows only |
| BLD-29 | none | PX-137, PX-138 | eval-bench | iteration | M3.9, IMP-EV-0250, IMP-EV-0251, M4.2, PX-109, IMP-EV-0274 | new rows only |
| BLD-30 | none | PX-139 | observability | release-critical | M10.1, PX-116, IMP-EV-0032 | new rows only |

The tier cell of a task with both old and new rows lists the tiers of each; the authoritative tier of a row is on its card. BLD-25 depends, in the task list, on BLD-15 and BLD-23. PX-082 to PX-086 are accepted and unchanged, so that dependency is a sequencing preference and not an edge of the graph; PX-083 already depends on the forge event path of PX-008 and PX-009.

### Row to requirements

| Row | Requirements | BLD |
|---|---|---|
| PX-099 | ADC-A01, ADC-A02 | BLD-02 |
| PX-100 | ADC-A03 | BLD-03 |
| PX-101 | ADC-A04 | BLD-04 |
| PX-102 | ADC-A05 | BLD-04 |
| PX-103 | ADC-A06 | BLD-05 |
| PX-104 | ADC-A07 | BLD-05 |
| PX-105 | ADC-A08 | BLD-07 |
| PX-106 | ADC-A09 | BLD-07 |
| PX-107 | ADC-B01 | BLD-08 |
| PX-108 | ADC-B02 | BLD-08 |
| PX-109 | ADC-B03 | BLD-09 |
| PX-110 | ADC-B04 | BLD-10 |
| PX-111 | ADC-B05 | BLD-11 |
| PX-112 | ADC-B06 | BLD-11 |
| PX-113 | ADC-B07 | BLD-12 |
| PX-114 | ADC-C01 | BLD-13 |
| PX-115 | ADC-C02 | BLD-14 |
| PX-116 | ADC-C03 | BLD-15 |
| PX-117 | ADC-C04 | BLD-16 |
| PX-118 | ADC-D01 | BLD-17 |
| PX-119 | ADC-D02 | BLD-18 |
| PX-120 | ADC-E01 | BLD-19 |
| PX-121 | ADC-E02 | BLD-19 |
| PX-122 | ADC-E03 | BLD-19 |
| PX-123 | ADC-E04 | BLD-20 |
| PX-124 | ADC-E05 | BLD-21 |
| PX-125 | ADC-F01 | BLD-23 |
| PX-126 | ADC-F02 | BLD-23 |
| PX-127 | ADC-F03 | BLD-23 |
| PX-128 | ADC-F04 | BLD-24 |
| PX-129 | ADC-F05 | BLD-24 |
| PX-130 | ADC-G01 | BLD-26 |
| PX-131 | ADC-G02 | BLD-26 |
| PX-132 | ADC-H01 | BLD-27 |
| PX-133 | ADC-I01 | BLD-28 |
| PX-134 | ADC-I02 | BLD-28 |
| PX-135 | ADC-I03 | BLD-28 |
| PX-136 | ADC-I04 | BLD-28 |
| PX-137 | ADC-J01 | BLD-29 |
| PX-138 | ADC-J02 | BLD-29 |
| PX-139 | ADC-J03 | BLD-30 |

## 8. Corrections to the draft mapping

The draft mapping written before the rows were read was checked against the actual rows of doc 62 and corrected as follows.

| BLD | Draft | Finding | Result |
|---|---|---|---|
| BLD-02 | PX-043, PX-048 | They serve and show the stream and own the registry and client input. No row has ACK, resize, a bounded ring or the agent tools `shell.input` and `shell.attach`. | Residual PX-099 |
| BLD-03 | PX-051, PX-053, PX-054, PX-056 | They cover the Core commands, the composer and the picker. Nothing lets the CLI or the IDE adapter set mode or preference. The audit's Verify mode is in no row. | Residual PX-100; Verify not adopted (section 9) |
| BLD-04 | PX-061, PX-062 plus pause and resume | PX-061 and PX-062 cover per-turn checkpoints, exact restore and fork. Pause and resume, named checkpoints, retention and collection are in no row. | New PX-101 and PX-102 |
| BLD-05 | PX-048, PX-061, PX-062 | PX-048 reuses the existing revision-bound review at the review boundary. Hunk attribution to the tool call, per-hunk decisions while the run continues and Review in every task state are in no row. | New PX-103 and PX-104 |
| BLD-06 | PX-044, PX-045 | PX-044 is the design foundation; PX-045 is the shell and not part of this task. | PX-044 only |
| BLD-17 | PX-065 | PX-065 is the lifecycle policy (creation, setup commands, cleanup, retention). It does not bind a task's run to its worktree through a task option. | Residual PX-118 |
| BLD-18 | PX-066 | PX-066 is apply-back of a worktree result to the user's checkout. The agent-facing `git.merge` tools over child branches, persisted merge state, post-merge verification and the parent completion gate are in no row. | Residual PX-119 |
| BLD-19 | PX-073 | PX-073 has the CDP deny list, a page-origin allow-list gate, certificate trust, permissions and view ownership. A network-target policy (loopback, private, link-local, metadata, rebinding), viewport capture, console, network, scroll, wait, the escalation field and latch for browser actions, the mutation observer, `since_fingerprint`, multi-frame trees and persistent identities are in no row. | Residual PX-120, PX-121 and PX-122 |

Confirmed as drafted: BLD-01 (PX-041, PX-042, PX-047), BLD-07 (PX-052 plus skills trust and index, now PX-105 and PX-106), BLD-22 (PX-069 to PX-076) and BLD-25 (PX-082 to PX-086).

## 9. Not adopted and out of scope

| Item | Why | Where it goes |
|---|---|---|
| Reviewer family enforcement, a conservative correlated feasibility bound and a no-checks policy for cascade and critique (BLD-28) | Each changes router semantics that ADR-R-049..056 and EPR-000..019 pin. The sealed text records reviewer family but does not require it; feasibility is specified as a confidence-adjusted floor; the owner asked for none of these. | An EPR Decision Record. PX-133 to PX-136 measure and conform only. |
| Policy Lab searches only the `auto` floor; profiler runs in shadow only (audit-I) | Router and rollout semantics. | An EPR Decision Record. |
| The Verify mode of BLD-03 | PX-051 defines AGENT, PLAN, DEBUG, MULTITASK and ASK; DEBUG makes reproduction mandatory. A goal-template Verify mode is a product choice not requested. | A later record if the owner wants it. |
| Workspace Editor, extension marketplace and the Customize surface beyond existing registries, desktop shell platform integration | DR-PX-2026-10-03-009, -011 and -012 are proposed and the owner did not ask for them. | Their records. |
| Inline edit, low-latency completion and Next Edit Ripple; cross-repository intelligence (REQ-EV-0156); a plugin marketplace | Non-goals of docs 10 and 29, a DEFERRED sealed row and an unratified record. | The owner, when asked. |
| Enterprise SSO behind the identity interface; the cloud-side policy gaps of audit-H item 5 (execution profile validation at the repository mapping, Organization-scope memory through the cloud store, many-repository mapping) | Not in the BLD list; PX-129 adds the OIDC and signed bundle path only. | A later record. |
| VER-01..08, FIX-01..21, DOC-01..04 | Verification, fixes and dossier tasks, not build behaviour. The fixes are on pull request #61 (PX-067 carries a delivered-in-part note for FIX-01). DOC-01 (restating overclaiming tasks), DOC-02 (docs ahead of code) and DOC-03 (`tools/evidence-check`) are dossier or tooling tasks that need their own `DOC-*` nodes. | Separate dossier tasks. |

## 10. Still unverified and owner inputs

- **Static findings.** Most audit findings were read, not run. The implementing task of every row starts with an existing-code audit on the merged state (doc 84) and records the classification; where the audit was wrong (as it was about `.modbit/` writes, which Phase 0 found already refused) the row narrows and says so.
- **Fixes are not merged.** FIX-01..21 live on pull request #61. Rows that build on them (PX-103 on FIX-06, PX-109 on FIX-07, PX-118 on FIX-01, PX-099 on FIX-09 and FIX-20, PX-114 and PX-139 on FIX-13 and FIX-14) confirm the merged state first. FIX-19 (browser) was only partly delivered and its Electron end-to-end test has not been run.
- **Inputs only the owner can supply.** A dependency admission naming the embedding model (PX-112); a GitHub test repository and token (PX-125, PX-126 for its live event and PX-127); first-party provider keys for the paired benchmark (PX-136, which uses the compatible gateway of DR-M9-002 until then); a threshold profile for gates A to G (no row here attests a gate); a signing identity and the SBOM record of M10.2 for anything that ships a helper. A row that lacks its input records BLOCKED with that reason and never claims a proof.
- **Decisions that stay with the owner.** Whether the learned embedder becomes the default (decided by PX-137, not assumed); whether the procedural projection becomes the default (decided by the trial in PX-114); whether any cascade or critique default is changed (an EPR record after PX-136).

