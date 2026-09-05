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
├─ Models/providers
├─ Execution locations
├─ Permissions/policy
├─ Browser permissions
├─ Memory
└─ Account / cloud
```

## Core screens

### 1. Home / Fleet
attention-first supervision, implemented as Modbit-native states:
- **Needs Attention**: approval, blocked credential, ambiguity, protected effect, conflict.
- **Ready for Review**: completed work with evidence and unresolved review decisions.
- **Running**: active turns/subagents.
- **Waiting**: waiting on external process, model quota, user-specified condition or queue capacity.
- **Completed**: accepted/merged/exported.
- **Failed**: terminal task failure after retry/recovery policy.

Cards show task goal, workspace, execution location, duration, active agent count, latest evidence, risk/effect indicator and next required action. Do not interrupt the user for routine progress.

### 2. New Task
Required inputs: goal, workspace/repository or general Work space. Optional advanced controls: branch/base revision, model policy, execution mode (`local_trusted` / `cloud_isolated`), permission profile, browser access, skill pack.

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
- evidence chain;
- merge/apply/export actions.

A green “done” state is impossible without the configured verification gate passing.

## User journeys

### Coding task
`Create task → acquire workspace snapshot/worktree → retrieve context → model turn → tool/procedural execution → edits → tests/diagnostics → repair loop if needed → review evidence → merge/apply`.

### Browser-backed engineering task
`Create task → browser capability grant → same live browser session opens → semantic actions → targeted visual fallback only when required → evidence capture → result/review`.

### Remote continuation
`Local task → user chooses Continue in Cloud → create checkpoint + handoff bundle → capability negotiation → cloud Core worker + isolated sandbox → event stream back to desktop → review/merge`.

### Restart/resume
`App/Core restart → load Session/Event Store → restore protocol state → verify checkpoint epoch → reconnect terminal/browser/sandbox if alive or rehydrate from checkpoint → continue at exact control state`.

## Accessibility

Keyboard navigation for all fleet/review/approval actions; semantic labels on agent/tool states; no color-only status; diff and terminal views expose text alternatives; browser takeover state announced; reduced motion respected.

## Product acceptance

The product is usable when a new user can clone/open a real repository, delegate a nontrivial change, observe real tool execution, survive restart, review a real diff and verification evidence, and accept the result without entering an IDE.

## Execution objectives and workflow progress

Auto mode offers Cost, Balance (default), Intelligence and permitted organization profiles. Each remains subject to the same quality floor, permissions, data policy and budget. Allowed manual model pins remain available for expert/reproducible work and report conflicts explicitly. Users see useful running/verifying/reviewing/escalating/needs-attention progress, full task cost and unresolved outcomes through Core projections. Model/workflow labels are policy-controlled but always internally auditable; development/admin diagnostics expose authorized reasons and versions. An economical draft is not presented as completed before required gates pass. See docs 15/27 and EPR-005 in doc 49.
