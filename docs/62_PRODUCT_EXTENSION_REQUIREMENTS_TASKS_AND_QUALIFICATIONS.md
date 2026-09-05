# Product extension requirements, tasks and qualifications

Additive ledger authorized by DR-PX-2026-09-05 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`). It never modifies the 291 base rows or the EPR ledger. `tools/dossier_px.py` parses it into the same graph node types as the base and EPR ledgers; it pins no count, so effective totals are whatever the rows sum to, and `tools/check_dossier.py` D9 enforces structure: contiguous IDs from 000, one task card and one `PX-E2E` scenario per ADOPT row, one qualification per row with the same owner, and DEFERRED rows with no task, milestone or prerequisites. Rows are appended by stage; earlier rows are never edited except by a new Decision Record.

Columns: requirement, title, task, qualification, canonical owner, milestone, release (ALPHA, BETA, RELEASE_ZERO or POST_ZERO for DEFERRED rows), disposition, prerequisites that must be COMPLETE first. A task joins the release named in its row and every later release; readiness is derived in `75_PHASED_RELEASE_PLAN_AND_READINESS.md`.

## Ledger

| Requirement | Title | Task | Qualification | Owner | Milestone | Release | Disposition | After |
|---|---|---|---|---|---|---|---|---|
| REQ-PX-000 | Headless CLI thin client for the task lifecycle | PX-000 | QUAL-PX-000 | desktop | M2 | ALPHA | ADOPT | M1.3,M2.7 |
| REQ-PX-001 | Thin-client conformance contract for external development-environment adapters | PX-001 | QUAL-PX-001 | desktop | M2 | ALPHA | ADOPT | PX-000 |
| REQ-PX-002 | VS Code adapter as a conformant thin client | PX-002 | QUAL-PX-002 | desktop | M6 | BETA | ADOPT | PX-001,M6.6 |
| REQ-PX-003 | JetBrains adapter after abstraction conformance | — | QUAL-PX-003 | desktop | — | POST_ZERO | DEFERRED | — |
| REQ-PX-004 | Provenance-bound external diagnostics intake | PX-004 | QUAL-PX-004 | context-engine | M6 | BETA | ADOPT | PX-001,M3.4 |
| REQ-PX-005 | Constrained inline patch through ChangeTransaction | PX-005 | QUAL-PX-005 | workspace-git | M6 | BETA | ADOPT | M2.1,M2.9 |
| REQ-PX-006 | GitHub forge adapter behind the External Tool Hub | PX-006 | QUAL-PX-006 | external-tools | M6 | BETA | ADOPT | M2.5,M6.5 |
| REQ-PX-007 | Pull request create and update from a reviewed result | PX-007 | QUAL-PX-007 | workspace-git | M6 | BETA | ADOPT | PX-006,M2.2 |
| REQ-PX-008 | Review-comment steering as untrusted durable input | PX-008 | QUAL-PX-008 | core-runtime | M9 | RELEASE_ZERO | ADOPT | PX-007,M6.5 |
| REQ-PX-009 | CI-result ingestion as provenance-bearing evidence | PX-009 | QUAL-PX-009 | verification | M9 | RELEASE_ZERO | ADOPT | PX-007,M2.8 |
| REQ-PX-010 | Issue-to-task intake from CLI and desktop | PX-010 | QUAL-PX-010 | desktop | M6 | BETA | ADOPT | PX-006,PX-000 |
| REQ-PX-011 | Forge webhook intake through the Cloud API | PX-011 | QUAL-PX-011 | sandbox-cloud | M8 | RELEASE_ZERO | ADOPT | PX-010,M8.1 |
| REQ-PX-012 | Team collaboration: shared task history and assignment | — | QUAL-PX-012 | desktop | — | POST_ZERO | DEFERRED | — |
| REQ-PX-013 | Team collaboration: comments and chat notifications | — | QUAL-PX-013 | desktop | — | POST_ZERO | DEFERRED | — |

## Qualifications

| Qualification | Requirement | Owner | Real qualification | Failure and negative proof |
|---|---|---|---|---|
| QUAL-PX-000 | REQ-PX-000 | desktop | Real Core plus the CLI process on a fixture repository: create a task, stream events by cursor as JSON lines, answer a question, approve one protected effect, kill and restart Core, resume by cursor; exit codes match the documented contract and exactly one effect receipt exists | Kill the CLI mid-stream and reconnect: no duplicate command, no duplicate effect. A forged command without the boot secret is rejected. The CLI binary contains no provider, filesystem, Git or policy code path: static dependency check and runtime tracing both prove every effect went through Core |
| QUAL-PX-001 | REQ-PX-001 | desktop | Conformance suite run against the CLI and the desktop protocol client on a real Core: command idempotency, cursor replay after disconnect, intent-hash-bound approval, attention reasons and acceptance verdict rendering, Core rejection paths; static dependency scan proves no provider, filesystem, Git or policy code in the client | A client that retries a command with a new id, replays without a cursor, or links a Git or provider library fails the suite and is not exposed |
| QUAL-PX-002 | REQ-PX-002 | desktop | Real VS Code extension host loading the adapter against a real local Core: create, steer, approve, review a fixture task from the editor; diagnostics forwarded with revision; restart the editor mid-task and resume by cursor | Adapter attempting to write the workspace, run a tool or read a secret has no code path and the Core rejects any forged command; a revision-mismatched diagnostics batch is discarded with an event |
| QUAL-PX-003 | REQ-PX-003 | desktop | Entry condition, not a current proof: the shared adapter library passes QUAL-PX-001 and QUAL-PX-002 on VS Code, then a JetBrains host passes the identical suite before any promotion Decision Record | Deferred; any JetBrains code before the entry condition is a release blocker under doc 73 |
| QUAL-PX-004 | REQ-PX-004 | context-engine | Real adapter submits language-service diagnostics for a fixture revision; Core normalizes them with provenance external_ide, they appear in Context Pack provenance and verification plan inputs, and a mandatory verification step still executes Modbit's own check | Submission for a stale revision is discarded; a diagnostics batch cannot mark any verification step passed; malformed batches are rejected before persistence |
| QUAL-PX-005 | REQ-PX-005 | workspace-git | From the review surface and from the CLI apply a one-hunk user edit to a real worktree through ChangeTransaction: revision precondition checked, provenance user_direct_edit recorded, workspace revision advanced, stale CodeReferences invalidated | Edit against a stale revision is refused; edit to a protected path is denied after symlink resolution; no client keeps an unsaved buffer |
| QUAL-PX-006 | REQ-PX-006 | external-tools | Real GitHub test repository through the forge adapter: read an issue, create and update a PR, read review comments and check-run status, each with capability lease, effect class, receipt and idempotency key; token supplied by the secret broker only | Call without lease or with a token in arguments is rejected; retried create with the same idempotency key yields one PR; egress to any other host is denied |
| QUAL-PX-007 | REQ-PX-007 | workspace-git | Reviewed fixture result opens a PR on a dedicated branch after approval; PR body carries the evidence summary; a later revision updates the same PR with a new receipt | Stale candidate revision cannot be pushed; denied approval leaves no branch on the remote; crash after push before receipt reconciles to exactly one PR |
| QUAL-PX-008 | REQ-PX-008 | core-runtime | Review comment from an allowed identity on the fixture PR becomes a TaskSteered event with provenance forge_review_comment and untrusted tagging, and the agent acts on it | Comment from a disallowed identity is recorded and ignored; comment text asking to approve an effect or widen capability changes nothing; injection suite passes |
| QUAL-PX-009 | REQ-PX-009 | verification | Real check-run results for the task branch are ingested as evidence artifacts with provider, run id, commit and OutputRef logs and appear in Review with provenance ci | A green CI run cannot mark a qualification PASS; a qualification naming a real test still runs in Modbit; mismatched commit is rejected |
| QUAL-PX-010 | REQ-PX-010 | desktop | From the CLI and the desktop New Task screen create a task from a real issue URL; issue text enters as untrusted context with provenance forge_issue and the task runs the ordinary loop | Issue text containing instructions cannot change policy or capabilities; unreadable issue yields a clear error and no task |
| QUAL-PX-011 | REQ-PX-011 | sandbox-cloud | Real GitHub App webhook to the staging Cloud API creates the same canonical task for the tenant after signature verification and policy; the desktop sees it by cursor | Unsigned or replayed webhook is rejected and audited; cross-tenant repository mapping is denied; no second task model exists |
| QUAL-PX-012 | REQ-PX-012 | desktop | Deferred proof: shared history and assignment appear as cloud projections readable by multiple authenticated clients with tenant isolation | Deferred; not on the Release Zero path |
| QUAL-PX-013 | REQ-PX-013 | desktop | Deferred proof: comments and notifications are durable events with provenance and never authority | Deferred; not on the Release Zero path |

## Task cards

<a id="px-000"></a>

## PX-000 — Headless CLI thin client for the task lifecycle

- **Requirement:** REQ-PX-000; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** M1.3, M2.7.
- **Scope and acceptance:** `modbit` command-line client speaking the authenticated local SurfaceProtocol: create task, steer, pause, resume, cancel, respond to questions, approve or deny effects, subscribe to events from a cursor, read OutputRefs and artifacts, print JSON lines and human text, and return documented exit codes. It is a thin client: no orchestration, context, memory, Git state, recovery, policy or tool execution lives in it, and it holds no provider credentials. Headless use in scripts and CI is the design target; the desktop app and the CLI are interchangeable clients of one Core.
- **Production wiring:** reuse the desktop's protocol client library (`packages/surface-protocol`) or a Rust twin of it; authenticate exactly as Electron main does; no new Core command types beyond those in `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`.
- **Real qualification:** QUAL-PX-000 / PX-E2E-000.
- **Failure and negative proof:** as in QUAL-PX-000; additionally cancellation from the CLI reconciles in-flight tools through the ordinary cancellation domains.
- **Evidence:** build digest, Core and CLI revisions, event cursor ranges, effect receipt id, exit codes, and the dependency check output under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-001"></a>

## PX-001 — Thin-client conformance contract for external development-environment adapters

- **Requirement:** REQ-PX-001; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** PX-000.
- **Scope and acceptance:** Define and implement the conformance suite every SurfaceProtocol client must pass: command idempotency, cursor replay, intent-hash-bound approvals, attention and verdict rendering, rejection paths, and a static proof that the client contains no provider, filesystem, Git or policy code path. Publish the contract as `packages/ide-adapter-core` and pass it with the CLI and desktop client (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`).
- **Production wiring:** reuse `packages/surface-protocol`; the suite runs against a real local Core in CI; no new Core commands.
- **Real qualification:** QUAL-PX-001 / PX-E2E-001.
- **Failure and negative proof:** as in QUAL-PX-001.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-002"></a>

## PX-002 — VS Code adapter as a conformant thin client

- **Requirement:** REQ-PX-002; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** PX-001, M6.6.
- **Scope and acceptance:** First IDE adapter built on `packages/ide-adapter-core`: task panel to create, steer, approve and review; forwards language-service diagnostics with revision provenance; shows results and evidence; owns nothing. Ships in Beta only after the CLI and desktop have proven the protocol in Alpha.
- **Production wiring:** real VS Code extension host; SurfaceProtocol over the same authenticated local transport; no editor features are replaced.
- **Real qualification:** QUAL-PX-002 / PX-E2E-002.
- **Failure and negative proof:** as in QUAL-PX-002; JetBrains follows only after this adapter and the shared library pass conformance (REQ-PX-003).
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-004"></a>

## PX-004 — Provenance-bound external diagnostics intake

- **Requirement:** REQ-PX-004; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M6 / BETA; **prerequisites:** PX-001, M3.4.
- **Scope and acceptance:** `SubmitExternalDiagnostics` normalizes IDE or linter diagnostics into the canonical diagnostics records with provenance external_ide, bound to workspace and file revision; used as context and verification-plan input, never as a substitute for a mandatory verification step.
- **Production wiring:** extend `crates/diagnostics` normalization and the SurfaceProtocol command set in doc 30; revision mismatch discards.
- **Real qualification:** QUAL-PX-004 / PX-E2E-004.
- **Failure and negative proof:** as in QUAL-PX-004.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-005"></a>

## PX-005 — Constrained inline patch through ChangeTransaction

- **Requirement:** REQ-PX-005; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M6 / BETA; **prerequisites:** M2.1, M2.9.
- **Scope and acceptance:** Allow a user to apply a small direct edit from the review surface or CLI exclusively through the canonical ChangeTransaction on the Workspace File Service: revision precondition, path policy after symlink resolution, provenance user_direct_edit, one event and revision advance, stale CodeReferences invalidated. No editor buffer model anywhere (`20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`).
- **Production wiring:** reuse `change.propose`/`change.apply` semantics via an `ApplyUserPatch` command; no new write path.
- **Real qualification:** QUAL-PX-005 / PX-E2E-005.
- **Failure and negative proof:** as in QUAL-PX-005.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-006"></a>

## PX-006 — GitHub forge adapter behind the External Tool Hub

- **Requirement:** REQ-PX-006; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** external-tools / M6 / BETA; **prerequisites:** M2.5, M6.5.
- **Scope and acceptance:** Implement the `forge.*` tool family for GitHub (`forge.issue.read`, `forge.pr.create`, `forge.pr.update`, `forge.pr.comments.read`, `forge.ci.status`) behind the existing External Tool Hub with capability leases, effect classes, receipts, idempotency keys and broker-supplied tokens (`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`).
- **Production wiring:** `crates/tools (external.*)` adapter; egress policy `api.github.com:443`; secret handle `github-token`; other forges later behind the same family.
- **Real qualification:** QUAL-PX-006 / PX-E2E-006.
- **Failure and negative proof:** as in QUAL-PX-006.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-007"></a>

## PX-007 — Pull request create and update from a reviewed result

- **Requirement:** REQ-PX-007; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M6 / BETA; **prerequisites:** PX-006, M2.2.
- **Scope and acceptance:** From Review or CLI, push the dedicated branch and open or update a PR as protected external effects with approval and receipts, bound to the exact candidate revision, with the evidence summary in the PR body.
- **Production wiring:** Change Engine merge/export path plus `forge.pr.*`; branch push through the typed Git operation; no hidden shell.
- **Real qualification:** QUAL-PX-007 / PX-E2E-007.
- **Failure and negative proof:** as in QUAL-PX-007.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-008"></a>

## PX-008 — Review-comment steering as untrusted durable input

- **Requirement:** REQ-PX-008; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M9 / RELEASE_ZERO; **prerequisites:** PX-007, M6.5.
- **Scope and acceptance:** Allowed reviewers' PR comments addressed to Modbit become durable TaskSteered events with provenance forge_review_comment and untrusted tagging through the ordinary steering path; identity allowlist from organization policy.
- **Production wiring:** `forge.pr.comments.read` polling or webhook (PX-011) feeding the existing Steer command; no new input channel type.
- **Real qualification:** QUAL-PX-008 / PX-E2E-008.
- **Failure and negative proof:** as in QUAL-PX-008.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-009"></a>

## PX-009 — CI-result ingestion as provenance-bearing evidence

- **Requirement:** REQ-PX-009; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M9 / RELEASE_ZERO; **prerequisites:** PX-007, M2.8.
- **Scope and acceptance:** Ingest check-run and workflow results for the task branch as evidence artifacts with provider, run id, commit and OutputRef logs; show them in Review with provenance ci; they inform the verification plan and never constitute an automatic qualification PASS.
- **Production wiring:** `forge.ci.status` into the Verification Engine's external-evidence class; qualification tests still execute in Modbit.
- **Real qualification:** QUAL-PX-009 / PX-E2E-009.
- **Failure and negative proof:** as in QUAL-PX-009.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-010"></a>

## PX-010 — Issue-to-task intake from CLI and desktop

- **Requirement:** REQ-PX-010; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** PX-006, PX-000.
- **Scope and acceptance:** `modbit task from-issue <url>` and the New Task screen pull a GitHub issue through `forge.issue.read` and create a canonical task with the issue as untrusted context and provenance forge_issue.
- **Production wiring:** thin clients call `CreateTask` with origin metadata; no second task model.
- **Real qualification:** QUAL-PX-010 / PX-E2E-010.
- **Failure and negative proof:** as in QUAL-PX-010.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-011"></a>

## PX-011 — Forge webhook intake through the Cloud API

- **Requirement:** REQ-PX-011; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** sandbox-cloud / M8 / RELEASE_ZERO; **prerequisites:** PX-010, M8.1.
- **Scope and acceptance:** A GitHub App webhook to the Cloud API creates the same canonical task for the tenant after signature verification and policy checks (`24_CLOUD_CONTROL_PLANE_AND_SYNC.md`); PR events feed PX-008/PX-009 when configured.
- **Production wiring:** Cloud API endpoint issuing the canonical CreateTask/Steer commands; replay protection and audit.
- **Real qualification:** QUAL-PX-011 / PX-E2E-011.
- **Failure and negative proof:** as in QUAL-PX-011.
- **Evidence:** build digest, Core and client revisions, event cursor ranges, effect receipt ids, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

## Real-system scenarios

### PX-E2E-000 — Headless CLI task lifecycle

**Setup:** packaged Core, CLI binary, real `ts-webapp` fixture, live provider test model.  
**Action:** from a shell, create a coding task, follow events, answer the agent's question, approve the protected write, then `SIGKILL` Core and resume following from the last cursor.  
**Pass:** the task reaches ReadyForReview with real tests passing; exactly one effect receipt; the CLI exit code is 0 on success and documented non-zero on cancel, denial or failure; no duplicate command after reconnect; the CLI process never opened the repository, a provider endpoint or the policy store directly.

### PX-E2E-001 — Thin-client conformance on CLI and desktop client

**Setup:** packaged Core, CLI, desktop protocol client, fixture repository.  
**Action:** run the conformance suite: retry commands, drop and replay the event stream, approve with a bound intent hash, force Core rejections, scan client dependencies.  
**Pass:** both clients pass every case; a deliberately broken client fails the suite.

### PX-E2E-002 — VS Code adapter drives a real task

**Setup:** real VS Code extension host with the adapter, real local Core, fixture repository, live provider test model.  
**Action:** create a task from the editor, forward diagnostics, approve the protected write, restart the editor mid-task.  
**Pass:** task completes with real tests; diagnostics appear with provenance; resume by cursor; no workspace write originated in the adapter.

### PX-E2E-004 — External diagnostics as evidence only

**Setup:** adapter submitting language-service diagnostics for a fixture revision.  
**Action:** submit valid, stale and malformed batches; run a task whose verification plan includes a mandatory typecheck.  
**Pass:** valid batch normalized with provenance external_ide; stale and malformed rejected with events; the mandatory typecheck still executes in Modbit.

### PX-E2E-005 — Inline patch through ChangeTransaction

**Setup:** review surface and CLI on a real worktree.  
**Action:** apply a one-hunk user edit, then retry against a stale revision, then target a protected path.  
**Pass:** first edit lands with provenance user_direct_edit and a revision advance; stale and protected attempts are refused; no client buffer state exists.

### PX-E2E-006 — Forge adapter with leases and receipts

**Setup:** real GitHub test repository, broker-held token.  
**Action:** read an issue, create and update a PR, read comments and check-run status, retry the create with the same idempotency key, attempt an off-allowlist host.  
**Pass:** each call has lease, effect class and receipt; one PR exists; egress denial recorded.

### PX-E2E-007 — Pull request from a reviewed result

**Setup:** completed fixture task in Review.  
**Action:** open a PR, approve the push, revise, update the PR; crash Core between push and receipt.  
**Pass:** exactly one PR with the evidence summary; update carries a new receipt; reconciliation after crash yields no duplicate.

### PX-E2E-008 — Review-comment steering

**Setup:** fixture PR with comments from allowed and disallowed identities, including an injection attempt.  
**Action:** ingest comments.  
**Pass:** allowed comment becomes TaskSteered with provenance and the agent acts; disallowed and injected comments change nothing and are audited.

### PX-E2E-009 — CI results are evidence, not verdicts

**Setup:** fixture branch with a green check-run and a qualification naming a real test.  
**Action:** ingest CI results; run the task's verification.  
**Pass:** CI evidence appears with provenance ci and OutputRef logs; the qualification is PASS only after Modbit's own test execution.

### PX-E2E-010 — Task from an issue

**Setup:** real issue containing instructions to ignore policy.  
**Action:** `modbit task from-issue <url>` and the desktop New Task path.  
**Pass:** task created with the issue as untrusted context; policy and capabilities unchanged; ordinary loop runs.

### PX-E2E-011 — Webhook intake through the Cloud API

**Setup:** staging Cloud API with a GitHub App installation on the fixture repository.  
**Action:** deliver a signed webhook, a replayed webhook and an unsigned webhook.  
**Pass:** one canonical task created for the tenant and visible to the desktop by cursor; replay and unsigned deliveries rejected and audited.
