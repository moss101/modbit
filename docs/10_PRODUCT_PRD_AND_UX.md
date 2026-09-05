# Product Requirements and UX Specification

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Product thesis

Modbit is an **agent-first engineering workspace** for users who want software work completed, verified and reviewable without living inside an IDE. It combines cowork-style task delegation with coding-specific workspace, Git, terminal, browser, test, context and evidence capabilities.

## Primary users

1. **Individual developer / technical founder** — delegates coding, debugging, repository analysis and browser-backed engineering work.
2. **Engineering lead** — supervises multiple concurrent tasks and reviews diffs/evidence rather than watching token streams.
3. **Platform / enterprise team** — needs policy, isolated execution, auditability, permissions, secrets and remote continuation.

## Goals

- Take a natural-language engineering task from request to verified code/artifact.
- Let one user supervise multiple agents with minimal interruption.
- Make every important result inspectable: code, commands, tests, browser actions, external effects and provenance.
- Resume accurately after renderer/Core/network/process interruption.
- Operate locally when possible; use cloud MicroVM execution when isolation, continuity or remote execution is required.
- Achieve retrieval/context efficiency competitive with best hybrid search while adding software-structure signals.

## Non-goals

- General-purpose IDE replacement.
- Pixel-only computer-use product.
- Unrestricted autonomous execution without policy or receipts.
- Separate local and cloud agent implementations.
- Model-training platform.
- A marketplace-driven architecture in P0.

## Navigation / information architecture

```text
Home
├─ New Task
├─ Needs Attention
├─ Ready for Review
├─ Running
├─ Waiting
├─ Completed
└─ Failed

Agents
├─ All Agents
└─ Agent profiles / capabilities

Workspaces
├─ Repositories / Spaces
├─ Branch/worktree status
└─ Engineering memory

Artifacts
├─ Diffs
├─ Reports
├─ Logs / OutputRefs
└─ Evidence receipts

Settings
├─ Models/providers and execution policy
├─ Execution locations
├─ Permissions/policy
├─ Browser permissions
├─ Memory
└─ Account / cloud
```

## Core screens

### 1. Home / Fleet
attention-first supervision, implemented as Modbit-native states:
- **Needs Attention**: approval, blocked credential, ambiguity, protected effect, conflict, human-required continuation of an execution plan, quality floor infeasible under current policy, budget exhausted with partial evidence.
- **Ready for Review**: completed work with evidence and unresolved review decisions.
- **Running**: active turns/subagents.
- **Waiting**: waiting on external process, model quota, user-specified condition or queue capacity.
- **Completed**: accepted/merged/exported.
- **Failed**: terminal task failure after retry/recovery policy.

Cards show task goal, workspace, execution location, objective profile, current execution phase (drafting, verifying, reviewing, escalating, awaiting human), duration, active agent count, latest evidence, risk/effect indicator and next required action. Do not interrupt the user for routine progress.

### 2. New Task
Required inputs: goal, workspace/repository or general Work space, or a forge issue URL whose text enters as untrusted context. The desktop, the headless CLI and IDE adapters are interchangeable thin clients of one Core. Optional advanced controls: branch/base revision, objective profile (Cost, Balance, Intelligence or an organization profile) or an allowed manual model pin, execution mode (`local_trusted` / `cloud_isolated`), permission profile, browser access, skill pack.

Submission creates a durable Session + Task before model invocation, so a crash after clicking Run is recoverable.

### 3. Task workspace
Three-column responsive layout:
- **Conversation / steering**: user goal, agent messages, questions, steer/pause/stop.
- **Work timeline**: RunSteps, subagents, tool activity, checkpoints, context provenance.
- **Live surface**: switches between Diff/Code Review, Terminal, Browser, Artifact and Evidence.

No editor chrome, explorer tree, extension host, debugger panels or IDE settings.

### 4. Trusted Code Review Surface
Read-only by default and bound to `{workspace_revision, file_revision}`. It supports syntax highlighting, symbol outline, line anchors, changed-line gutter, side-by-side/unified diff, diagnostics, test links and evidence references. Stale CodeReferences are visibly invalidated after revision changes.

### 5. Browser surface
The exact session controlled by the agent is shown. User can dock, expand, pop out, request screenshot, or take control. Takeover transfers the control lease; the agent observes but cannot inject input until lease is returned.

### 6. Review
Contains:
- goal/result summary;
- changed files and risk classification;
- tests/verification actually executed;
- unresolved diagnostics;
- external effects and approvals;
- the execution path actually taken (initial leg, escalation, independent review, human approval) with the acceptance verdict and the assurance level it had to meet;
- reviewer findings and how each was resolved;
- repair history, quarantined flaky checks, regression attribution against the pre-change baseline, scope expansions against the original plan and diff-invariant findings (`64_VERIFICATION_EXECUTION_CONTRACTS.md`);
- complete cost and time by leg, including reviewer tool cost;
- evidence chain;
- merge/apply/export actions, and open or update a pull request as a protected effect with a receipt (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`).

A green “done” state is impossible without the configured verification gate passing, and impossible while the acceptance verdict is REJECT or INCONCLUSIVE.

## User journeys

### Coding task
`Create task → acquire workspace snapshot/worktree → retrieve context → model turn → tool/procedural execution → edits → tests/diagnostics → repair loop if needed → review evidence → merge/apply`.

### Browser-backed engineering task
`Create task → browser capability grant → same live browser session opens → semantic actions → targeted visual fallback only when required → evidence capture → result/review`.

### Remote continuation
`Local task → user chooses Continue in Cloud → create checkpoint + handoff bundle → capability negotiation → cloud Core worker + isolated sandbox → event stream back to desktop → review/merge`.

### Restart/resume
`App/Core restart → load Session/Event Store → restore protocol state → verify checkpoint epoch → reconnect terminal/browser/sandbox if alive or rehydrate from checkpoint → continue at exact control state`.

## Flows, states, notifications and budgets

Every screen above has empty, loading, populated, error, degraded and recovery states, a notification model that interrupts only for attention, completion and failure, a complete keyboard model and interaction budgets, all specified and measured in `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`.

## Languages and platforms

Language support is a tiered, earned claim and platform support is separate from CI compatibility; both are defined in `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`. Alpha ships on macOS with TypeScript/JavaScript, Python and Rust at the Alpha baseline, and every client shows the language and platform state honestly.

## Accessibility

Keyboard navigation for all fleet/review/approval actions; semantic labels on agent/tool states; no color-only status; diff and terminal views expose text alternatives; browser takeover state announced; reduced motion respected.

## Product acceptance

A new user reaches a first useful task, meaning `ReadyForReview` with a real diff, test run and receipts on a small repository, within five minutes of first launch at the median on reference hardware; this is measured by PX-E2E-022 (`39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`). Beyond that, the product is usable when a new user can clone/open a real repository, delegate a nontrivial change, observe real tool execution, survive restart, review a real diff and verification evidence, see why the result was accepted, escalated or independently reviewed and what it cost, and accept the result without entering an IDE.

## Execution policy in the product

**Objective modes.** Auto mode offers Cost, Balance (default), Intelligence and permitted organization profiles. Every mode is subject to the same confidence-adjusted quality floor, permissions, data policy and budget; the user never has to pick a model to benefit from Auto. Allowed manual model pins remain available for expert, debugging, evaluation and reproducibility work; a pin still runs inside Modbit safety, context, tool, verification and cost boundaries and reports conflicts with policy explicitly.

**What the user sees.** Progress is projected from Core events as drafting, verifying, reviewing, escalating, awaiting human or needs-attention, together with complete task cost and unresolved outcomes. An economical draft is never presented as completed before the required gates pass. When no plan clears the quality floor, the task shows that the floor was infeasible under current policy and continues with the best eligible plan without claiming the target was met. Human-required continuations appear as Needs Attention items with the exact approval requested.

**Review.** The Review screen shows the executed path (initial leg, escalation, independent review, human approval), the acceptance verdict with the assurance level it had to meet, reviewer findings and their resolution, and cost by leg. The dossier labels DIRECT, CASCADE and CRITIQUE may appear as path labels where policy permits; they are never user choices.

**Labels and diagnostics.** The effective model and workflow are always logged internally. UI visibility of model and workflow labels is policy-controlled; development, canary and enterprise debugging expose authorized reasons and versions through routing diagnostics. A hidden label never makes routing unauditable.

**Organization controls** (policy, not UI toggles): permitted objective modes; permitted model and provider families; data residency; maximum request budget; maximum reasoning effort; repositories or paths where independent review is mandatory; whether escalation continuations are allowed; whether manual pins are allowed; routing telemetry retention; label visibility. These arrive as part of the signed policy bundle (`24_CLOUD_CONTROL_PLANE_AND_SYNC.md`) and are enforced by Core, never by the renderer.

Contracts and events: `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`; renderer projections: `32_DESKTOP_FRONTEND_IMPLEMENTATION.md`; architecture: docs 15/27; proof: EPR-005/007/010 in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`.
