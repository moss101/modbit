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
| REQ-PX-014 | Understanding and planning contract | PX-014 | QUAL-PX-014 | core-runtime | M2 | ALPHA | ADOPT | M2.7 |
| REQ-PX-015 | Retrieval-before-edit contract | PX-015 | QUAL-PX-015 | context-engine | M3 | BETA | ADOPT | M3.7,PX-014 |
| REQ-PX-016 | Change strategy contract | PX-016 | QUAL-PX-016 | workspace-git | M2 | ALPHA | ADOPT | M2.1,M2.2,PX-014 |
| REQ-PX-017 | Verification plan derivation contract | PX-017 | QUAL-PX-017 | verification | M2 | ALPHA | ADOPT | M2.8,PX-014 |
| REQ-PX-018 | Bounded evidence-driven repair loop with RepairAttempt records | PX-018 | QUAL-PX-018 | core-runtime | M2 | ALPHA | ADOPT | PX-017 |
| REQ-PX-019 | Self-review and completion contract | PX-019 | QUAL-PX-019 | verification | M2 | ALPHA | ADOPT | PX-018 |
| REQ-PX-020 | Fixed M2 competence baseline on public and internal suites | PX-020 | QUAL-PX-020 | eval-bench | M3 | BETA | ADOPT | M2.9,PX-019,M3.9 |
| REQ-PX-021 | Competence regression gate and targets after baseline | PX-021 | QUAL-PX-021 | eval-bench | M10 | RELEASE_ZERO | ADOPT | PX-020,M10.4 |
| REQ-PX-022 | Onboarding to a first useful task within five minutes | PX-022 | QUAL-PX-022 | desktop | M2 | ALPHA | ADOPT | M1.4,M2.9 |
| REQ-PX-023 | Screen flow and state completeness with notification model | PX-023 | QUAL-PX-023 | desktop | M6 | BETA | ADOPT | M6.6,PX-022 |
| REQ-PX-024 | Keyboard model and accessibility conformance | PX-024 | QUAL-PX-024 | desktop | M6 | BETA | ADOPT | M6.6 |
| REQ-PX-025 | Interaction budgets enforced in packaged E2E | PX-025 | QUAL-PX-025 | desktop | M10 | RELEASE_ZERO | ADOPT | M10.4,PX-023 |
| REQ-PX-026 | Alpha language baseline for TypeScript/JavaScript, Python and Rust | PX-026 | QUAL-PX-026 | verification | M2 | ALPHA | ADOPT | M2.8 |
| REQ-PX-027 | Language tier conformance suites A, B and C | PX-027 | QUAL-PX-027 | verification | M3 | BETA | ADOPT | M3.3,M3.4,PX-026 |
| REQ-PX-028 | Tier A conformance for TypeScript/JavaScript, Python and Rust | PX-028 | QUAL-PX-028 | context-engine | M3 | BETA | ADOPT | PX-027 |
| REQ-PX-029 | Explicit degradation path for Tier C and Unsupported languages | PX-029 | QUAL-PX-029 | context-engine | M3 | BETA | ADOPT | PX-027 |
| REQ-PX-030 | Platform CI compatibility matrix from M0, never release-grade by itself | PX-030 | QUAL-PX-030 | governance | M0 | ALPHA | ADOPT | M0.1 |
| REQ-PX-031 | Desktop platform release promotion by platform-specific E2E | PX-031 | QUAL-PX-031 | desktop | M10 | RELEASE_ZERO | ADOPT | M10.3,PX-030 |

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
| QUAL-PX-014 | REQ-PX-014 | core-runtime | Real fixture task: the agent records a plan through plan.update before the first write, asks exactly one typed question on an ambiguous fixture and none on an unambiguous one, and plan revisions appear as events in the timeline | A write before any plan is rejected by Core; a question that merely confirms verifiable repository facts is flagged by the internal suite |
| QUAL-PX-015 | REQ-PX-015 | context-engine | Real fixture task: every edited file has a retrieval record at the current workspace revision and the Context Ledger records use; symbol edits show definition and reference retrieval | An edit to a file without a retrieval record is rejected as a ToolCallPolicyDecision; a stale-revision retrieval does not satisfy the rule |
| QUAL-PX-016 | REQ-PX-016 | workspace-git | Real fixture task with a test harness: failing test written first, then the change; diffs are revision-bound ChangeTransactions with one concern each; a file outside the plan triggers a plan revision event | Silent scope widening without a plan revision is rejected; lockfile edited by hand rather than by its generator is flagged |
| QUAL-PX-017 | REQ-PX-017 | verification | Real fixture: derived verification plan recorded before the first run with build, typecheck, targeted tests, diagnostics delta and diff invariants; agent-added checks appear; mandatory checks cannot be removed | Attempt to delete a mandatory check is rejected; a task that fails still has its recorded plan |
| QUAL-PX-018 | REQ-PX-018 | core-runtime | Seeded failing fixture: each repair attempt records failure signature, hypothesis, evidence, intended fix, change ref and verification result before the next run; a repeated equivalent hypothesis triggers escalation or Needs Attention with the attempt history; attempt bounds from policy are enforced | A change after a failed verification without a RepairAttempt is rejected; equivalent hypothesis executed twice is impossible; bound exhaustion never truncates silently; WORSENED attempts are reverted or justified |
| QUAL-PX-019 | REQ-PX-019 | verification | Real fixture: the SelfReview step lists plan coverage, executed verifications, receipts, scope and leftovers; unresolved findings block the completion proposal; the Acceptance Gate, not the agent, decides completion | A completion proposal without a SelfReview or with unresolved findings is rejected; agent text claiming done changes nothing |
| QUAL-PX-020 | REQ-PX-020 | eval-bench | Both suites run under the frozen protocol on the real M2 product with the direct configuration; the immutable baseline bundle (digests, model metadata, per-task results, intervals) is recorded and referenced by digest | A baseline with gold-patch access, unpinned images or missing trial counts is rejected; no target may be recorded before this baseline exists |
| QUAL-PX-021 | REQ-PX-021 | eval-bench | Targets set by Decision Record per metric and tier after the baseline; the release candidate competence gate fails on regression beyond approved thresholds with intervals, including cost-improving routing changes that regress competence | A target recorded without a baseline digest is rejected; a candidate that regresses first-pass success or repair-loop distribution cannot pass the gate regardless of routing savings |
| QUAL-PX-022 | REQ-PX-022 | desktop | Playwright drives the packaged app on fresh profiles through provider setup, repository trust and a starter task on a small real repository with a live provider test model; median time to ReadyForReview with real evidence is within five minutes on reference hardware; p90 reported | Invalid key, network failure and untrusted repository each show the named cause and next action; a profile that skips provider setup cannot start a task; no step shows an outcome the Core has not persisted |
| QUAL-PX-023 | REQ-PX-023 | desktop | Each screen's empty, loading, populated, error, degraded and recovery states are forced against a real Core (Core restart, provider down, stale bundle, offline, unknown outcome, quality floor infeasible, human continuation) and each names cause, next action and evidence; notifications fire only for attention, completion and failure and coalesce per task | A screen missing a required state fails the matrix; routine progress producing a notification fails; a recovery banner claiming progress not in Core events fails |
| QUAL-PX-024 | REQ-PX-024 | desktop | Every screen traversed by keyboard only in the packaged app; global shortcuts, list navigation, review navigation and approval with confirmation work; accessibility suite passes; focus retained across state changes; live regions announce attention changes | Any control unreachable by keyboard, any color-only status, or lost focus on a Core event fails |
| QUAL-PX-025 | REQ-PX-025 | desktop | Packaged E2E asserts every interaction budget in doc 39 on reference hardware with Playwright traces and Core event timestamps | A budget miss on a release-critical path fails the candidate; timings taken from screenshots rather than traces are rejected |
| QUAL-PX-026 | REQ-PX-026 | verification | On the ts-webapp, python-service and rust-cli fixtures: exact and BM25 retrieval, revision-bound edits and real compiler and test-runner evidence attributed to tasks; clients label the languages at the Alpha baseline, not Tier A | Any structural or language-service claim in Alpha for these languages fails; an edit that corrupts encoding or line endings fails |
| QUAL-PX-027 | REQ-PX-027 | verification | Tier A, B and C conformance suites exist and run on real fixture repositories: symbol/reference recall and diagnostics parity for A, symbol extraction and build-output diagnostics for B, text safety and configured-command evidence for C; a language enters a tier only through a recorded pass | A language listed in a tier without a recorded suite pass is a release blocker; a grammar alone cannot classify a language |
| QUAL-PX-028 | REQ-PX-028 | context-engine | TypeScript/JavaScript, Python and Rust pass the Tier A suite with real headless language services on the fixtures; incremental index latency within budget; competence suite tasks for each language pass at baseline | Diagnostics parity failure or missing references on fixtures blocks the tier; a dead language service degrades explicitly rather than faking results |
| QUAL-PX-029 | REQ-PX-029 | context-engine | A fixture with an Unsupported language and one with a Tier C language: retrieval falls back to text, verification uses only configured commands, the plan states the limitation, every client shows the language state, edits to the Unsupported language require explicit per-task opt-in with provenance | Any structural claim, silent fallback or edit without opt-in fails |
| QUAL-PX-030 | REQ-PX-030 | governance | CI builds Core, CLI and runtime on macOS, Windows and Linux from M0 and runs unit, component and platform conformance suites (PTY/process, language services, Git, path policy, secrets, packaging, browser host); results labeled CI_COMPATIBLE only | Documentation, app or CLI text describing a CI-compatible platform as supported fails; a failing platform suite blocks the merge, not the label |
| QUAL-PX-031 | REQ-PX-031 | desktop | macOS reaches RELEASE_GRADE by the packaged desktop E2E catalog and the applicable Release Zero subset; Windows and Linux stay CI_COMPATIBLE until their own packaged E2E passes and a Decision Record records promotion | Promotion without platform E2E evidence is rejected; Release Zero on macOS does not imply any other platform |

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

<a id="px-014"></a>

## PX-014 — Understanding and planning contract

- **Requirement:** REQ-PX-014; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** M2.7.
- **Scope and acceptance:** Clarification policy and plan artifact per `28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §1: a plan recorded through plan.update before the first write, typed questions only when the change set, verification or a protected effect depends on the answer, plan revisions as events.
- **Production wiring:** WorkGraph plan state and `plan.get/update` tools; Core rejects writes before a plan; `PlanRecorded`/`PlanRevised` events.
- **Real qualification:** QUAL-PX-014 / PX-E2E-014.
- **Failure and negative proof:** as in QUAL-PX-014.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-015"></a>

## PX-015 — Retrieval-before-edit contract

- **Requirement:** REQ-PX-015; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** M3.7, PX-014.
- **Scope and acceptance:** No edit without a retrieval record for the file at the current workspace revision; symbol edits require definition and reference retrieval; Context Ledger records retrieval and later use (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §2).
- **Production wiring:** Context Engine retrieval records joined to ChangeTransaction preconditions; policy decision on violation.
- **Real qualification:** QUAL-PX-015 / PX-E2E-015.
- **Failure and negative proof:** as in QUAL-PX-015.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-016"></a>

## PX-016 — Change strategy contract

- **Requirement:** REQ-PX-016; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** workspace-git / M2 / ALPHA; **prerequisites:** M2.1, M2.2, PX-014.
- **Scope and acceptance:** Small revision-bound diffs, one concern per ChangeTransaction where possible, tests first where a harness exists, explicit plan revision on scope change, generated files only via generators (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §3).
- **Production wiring:** Change Engine transaction metadata carries plan linkage; scope check against the plan's expected set.
- **Real qualification:** QUAL-PX-016 / PX-E2E-016.
- **Failure and negative proof:** as in QUAL-PX-016.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-017"></a>

## PX-017 — Verification plan derivation contract

- **Requirement:** REQ-PX-017; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8, PX-014.
- **Scope and acceptance:** Verification plan derived from task type, changed files, repository configuration and agent proposals, recorded before the first run; agent may add, never remove, mandatory checks (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §4).
- **Production wiring:** Verification Engine plan materialization already specified in doc 33, now recorded as an event and bound to the plan.
- **Real qualification:** QUAL-PX-017 / PX-E2E-017.
- **Failure and negative proof:** as in QUAL-PX-017.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-018"></a>

## PX-018 — Bounded evidence-driven repair loop with RepairAttempt records

- **Requirement:** REQ-PX-018; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** core-runtime / M2 / ALPHA; **prerequisites:** PX-017.
- **Scope and acceptance:** RepairAttempt record per attempt (failure signature, hypothesis, evidence, intended fix, change ref, verification result, outcome); equivalent repeated hypotheses escalate through compiled slots or Needs Attention; policy bounds per failure signature and per task; WORSENED attempts reverted or justified (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §5).
- **Production wiring:** Core-runtime loop control with `RepairAttemptRecorded`/`RepairEscalated` events and `repair_attempts` persistence; escalation via EPR slots when present.
- **Real qualification:** QUAL-PX-018 / PX-E2E-018.
- **Failure and negative proof:** as in QUAL-PX-018.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-019"></a>

## PX-019 — Self-review and completion contract

- **Requirement:** REQ-PX-019; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** PX-018.
- **Scope and acceptance:** SelfReview step with structured findings before any completion proposal; unresolved findings block the proposal; the Acceptance Gate decides completion (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §6).
- **Production wiring:** Verification Engine consumes the SelfReview; `SelfReviewRecorded` event; completion proposal gated.
- **Real qualification:** QUAL-PX-019 / PX-E2E-019.
- **Failure and negative proof:** as in QUAL-PX-019.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-020"></a>

## PX-020 — Fixed M2 competence baseline on public and internal suites

- **Requirement:** REQ-PX-020; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / release:** eval-bench / M3 / BETA; **prerequisites:** M2.9, PX-019, M3.9.
- **Scope and acceptance:** Run the public benchmark and the internal competence suite under the frozen protocol of `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md` on the real M2 product with the direct configuration and record the immutable baseline bundle.
- **Production wiring:** Eval Harness under `benchmarks/agent-engineering`; fixture repositories from doc 50; baseline artifacts in the object store by digest.
- **Real qualification:** QUAL-PX-020 / PX-E2E-020.
- **Failure and negative proof:** as in QUAL-PX-020.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-021"></a>

## PX-021 — Competence regression gate and targets after baseline

- **Requirement:** REQ-PX-021; **related preserved requirements:** REQ-EV-0029.
- **Owner / milestone / release:** eval-bench / M10 / RELEASE_ZERO; **prerequisites:** PX-020, M10.4.
- **Scope and acceptance:** Targets by Decision Record per metric and tier only after the baseline; release-candidate competence gate with intervals; cost-improving routing changes that regress competence fail (doc 63).
- **Production wiring:** Release gate tooling and Policy Lab reporting; thresholds versioned with the release profile.
- **Real qualification:** QUAL-PX-021 / PX-E2E-021.
- **Failure and negative proof:** as in QUAL-PX-021.
- **Evidence:** build digest, Core revision, event cursor ranges, plan/RepairAttempt/SelfReview refs, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-022"></a>

## PX-022 — Onboarding to a first useful task within five minutes

- **Requirement:** REQ-PX-022; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** M1.4, M2.9.
- **Scope and acceptance:** Welcome, provider setup through the OS keychain with a live test call, explicit scoped repository trust with background indexing, starter-task gallery and free-text goal, first Review; median under five minutes to ReadyForReview with real evidence as defined in `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`.
- **Production wiring:** renderer `app-shell/` and `settings/` modules; provider test call through Core; templates from the detected stack; no client-side state beyond acknowledgements.
- **Real qualification:** QUAL-PX-022 / PX-E2E-022.
- **Failure and negative proof:** as in QUAL-PX-022.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-023"></a>

## PX-023 — Screen flow and state completeness with notification model

- **Requirement:** REQ-PX-023; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** M6.6, PX-022.
- **Scope and acceptance:** Empty, loading, populated, error, degraded and recovery states for every screen per the matrix in `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`; error copy names cause, next action and evidence; notifications only for attention, completion and failure, coalesced per task, deep-linked, OS delivery opt-in with quiet hours.
- **Production wiring:** renderer state machines fed by Core events; OS notifications through Electron main; the same reasons exposed to CLI and IDE adapters.
- **Real qualification:** QUAL-PX-023 / PX-E2E-023.
- **Failure and negative proof:** as in QUAL-PX-023.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-024"></a>

## PX-024 — Keyboard model and accessibility conformance

- **Requirement:** REQ-PX-024; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M6 / BETA; **prerequisites:** M6.6.
- **Scope and acceptance:** Global, list and review shortcuts; approval with confirmation for irreversible effects; focus retention; live regions; no color-only status; accessibility suite in packaged E2E (`39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md`).
- **Production wiring:** renderer focus management and shortcut registry; Playwright accessibility checks.
- **Real qualification:** QUAL-PX-024 / PX-E2E-024.
- **Failure and negative proof:** as in QUAL-PX-024.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-025"></a>

## PX-025 — Interaction budgets enforced in packaged E2E

- **Requirement:** REQ-PX-025; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M10.4, PX-023.
- **Scope and acceptance:** Assert the interaction budgets table of `39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md` in packaged E2E from Playwright traces and Core event timestamps; misses fail release-critical candidates.
- **Production wiring:** performance regression gates of M10.4 extended with UI budgets; results in the release evidence bundle.
- **Real qualification:** QUAL-PX-025 / PX-E2E-025.
- **Failure and negative proof:** as in QUAL-PX-025.
- **Evidence:** build digest, Core and renderer revisions, Playwright traces, Core event timestamps, run ids and artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

<a id="px-026"></a>

## PX-026 — Alpha language baseline for TypeScript/JavaScript, Python and Rust

- **Requirement:** REQ-PX-026; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M2 / ALPHA; **prerequisites:** M2.8.
- **Scope and acceptance:** Prove the three Alpha candidates at Tier C plus real compile and test evidence on the fixture stacks, with honest labels in every client (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** Verification Engine command evidence and the exact/BM25 paths of M2; labels via Core projections.
- **Real qualification:** QUAL-PX-026 / PX-E2E-026.
- **Failure and negative proof:** as in QUAL-PX-026.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-027"></a>

## PX-027 — Language tier conformance suites A, B and C

- **Requirement:** REQ-PX-027; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** verification / M3 / BETA; **prerequisites:** M3.3, M3.4, PX-026.
- **Scope and acceptance:** Implement the tier suites in `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md` over real fixture repositories and make tier entry a recorded pass; add the suites to `56_TOOL_CAPABILITY_CONFORMANCE.md`.
- **Production wiring:** Eval Harness plus verification fixtures; recorded promotion artifacts.
- **Real qualification:** QUAL-PX-027 / PX-E2E-027.
- **Failure and negative proof:** as in QUAL-PX-027.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-028"></a>

## PX-028 — Tier A conformance for TypeScript/JavaScript, Python and Rust

- **Requirement:** REQ-PX-028; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** PX-027.
- **Scope and acceptance:** Pass the Tier A suite for the three languages with real headless language services, incremental index latency within budget and competence baseline tasks passing (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** `crates/diagnostics` language-service adapters and `crates/retrieval` structural indexes of M3.
- **Real qualification:** QUAL-PX-028 / PX-E2E-028.
- **Failure and negative proof:** as in QUAL-PX-028.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-029"></a>

## PX-029 — Explicit degradation path for Tier C and Unsupported languages

- **Requirement:** REQ-PX-029; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** context-engine / M3 / BETA; **prerequisites:** PX-027.
- **Scope and acceptance:** Visible, explicit degradation: text retrieval, configured-command verification only, plan states the limitation, language state shown in every client, per-task opt-in with provenance for edits to Unsupported languages (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** Context Engine language classification joined to the plan and verification plan; projection field on tasks.
- **Real qualification:** QUAL-PX-029 / PX-E2E-029.
- **Failure and negative proof:** as in QUAL-PX-029.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-030"></a>

## PX-030 — Platform CI compatibility matrix from M0, never release-grade by itself

- **Requirement:** REQ-PX-030; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** governance / M0 / ALPHA; **prerequisites:** M0.1.
- **Scope and acceptance:** CI builds and platform conformance suites on macOS, Windows and Linux from M0; results labeled CI_COMPATIBLE; no documentation or client text presents CI compatibility as support (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** CI matrix in the monorepo of M0.1 plus the platform conformance suites; label enforcement in docs and clients.
- **Real qualification:** QUAL-PX-030 / PX-E2E-030.
- **Failure and negative proof:** as in QUAL-PX-030.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.
<a id="px-031"></a>

## PX-031 — Desktop platform release promotion by platform-specific E2E

- **Requirement:** REQ-PX-031; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M10 / RELEASE_ZERO; **prerequisites:** M10.3, PX-030.
- **Scope and acceptance:** macOS reaches RELEASE_GRADE through the packaged desktop E2E catalog and the applicable Release Zero subset; other platforms are promoted only by their own packaged E2E evidence and a Decision Record (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`).
- **Production wiring:** release gate tooling records platform promotions with evidence bundles.
- **Real qualification:** QUAL-PX-031 / PX-E2E-031.
- **Failure and negative proof:** as in QUAL-PX-031.
- **Evidence:** build digest, revisions, suite run ids, per-language and per-platform results, artifact digests under the existing evidence rules.
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

### PX-E2E-014 — Plan before write

**Setup:** fixture with one ambiguous and one unambiguous task.  
**Action:** run both.  
**Pass:** plan recorded before the first write in both; exactly one typed question on the ambiguous task; a forced write before plan is rejected.

### PX-E2E-015 — No edit without retrieval

**Setup:** fixture task touching a symbol used in three files.  
**Action:** run the task; then force an edit to an unretrieved file.  
**Pass:** retrieval records exist for every edited file at the current revision; the forced edit is rejected with a policy decision.

### PX-E2E-016 — Tests first, scope disciplined

**Setup:** `ts-webapp` behavior change with an existing test harness.  
**Action:** run the task; induce a change outside the plan.  
**Pass:** failing test precedes the change; the out-of-plan file triggers a plan revision event; lockfile untouched by hand.

### PX-E2E-017 — Verification plan recorded first

**Setup:** fixture task that will fail verification.  
**Action:** run; attempt to remove a mandatory check.  
**Pass:** derived plan recorded before the first run and retained after failure; removal rejected.

### PX-E2E-018 — Bounded repair with escalation on repeated hypothesis

**Setup:** seeded failure whose obvious fix does not work.  
**Action:** run with policy bounds of two attempts per signature.  
**Pass:** each attempt has a complete RepairAttempt record; the second equivalent hypothesis is not executed and the task escalates or moves to Needs Attention with history; a WORSENED attempt is reverted.

### PX-E2E-019 — Self-review gates the completion proposal

**Setup:** fixture task completed with one leftover debug statement.  
**Action:** let the agent propose completion.  
**Pass:** SelfReview finds the leftover; proposal blocked until resolved; Acceptance Gate decides afterwards.

### PX-E2E-020 — Baseline bundle

**Setup:** real M2 product, frozen protocol, both suites.  
**Action:** run the baseline.  
**Pass:** immutable bundle with digests, model metadata, trials, per-task results and intervals; attempt with gold-patch access rejected.

### PX-E2E-021 — Regression gate

**Setup:** baseline bundle and a candidate build with a seeded competence regression and a routing cost improvement.  
**Action:** run the gate.  
**Pass:** candidate fails the competence gate; a target recorded without a baseline digest is rejected.

### PX-E2E-022 — Five-minute onboarding

**Setup:** fresh OS user profile, packaged app, live provider test key, small real repository.  
**Action:** Playwright completes welcome, provider setup, repository trust and a starter task.  
**Pass:** median under five minutes to ReadyForReview with a real diff and test run; failure paths show cause and next action; no outcome shown before Core persisted it.

### PX-E2E-023 — State matrix

**Setup:** packaged app against a real Core with fault injection.  
**Action:** force Core restart, provider outage, stale bundle, offline, unknown outcome, quality floor infeasible and human continuation on each screen.  
**Pass:** every required state present with cause, next action and evidence; notifications only for attention, completion and failure.

### PX-E2E-024 — Keyboard and accessibility

**Setup:** packaged app.  
**Action:** traverse every screen and action by keyboard; run the accessibility suite.  
**Pass:** all controls reachable, focus retained across Core events, live regions announce attention, no color-only status.

### PX-E2E-025 — Interaction budgets

**Setup:** packaged app on reference hardware.  
**Action:** measure each budgeted interaction from traces and Core timestamps.  
**Pass:** all budgets met at p95; a seeded regression fails the candidate.

### PX-E2E-026 — Alpha language baseline

**Setup:** ts-webapp, python-service and rust-cli fixtures on the Alpha product.  
**Action:** run a task per fixture.  
**Pass:** text retrieval, safe edits and real compiler and test evidence; clients label the languages at the Alpha baseline with no structural claim.

### PX-E2E-027 — Tier suites gate classification

**Setup:** tier suites and a language with a grammar but no recorded pass.  
**Action:** run the suites; attempt to list the language in Tier B.  
**Pass:** suites run on real fixtures; the unrecorded listing is rejected as a release blocker.

### PX-E2E-028 — Tier A for the three languages

**Setup:** real headless language services on the fixtures.  
**Action:** run the Tier A suite and the per-language competence baseline tasks.  
**Pass:** recall and diagnostics parity thresholds met; incremental index latency within budget; dead language service degrades explicitly.

### PX-E2E-029 — Explicit degradation

**Setup:** fixtures in an Unsupported language and a Tier C language.  
**Action:** run tasks that touch them.  
**Pass:** text retrieval and configured-command verification only; plan states the limitation; language state visible in desktop and CLI; Unsupported edit requires per-task opt-in.

### PX-E2E-030 — Platform CI matrix

**Setup:** CI runners for macOS, Windows and Linux.  
**Action:** build Core, CLI and runtime; run platform conformance suites; scan docs and client strings.  
**Pass:** results labeled CI_COMPATIBLE; any text presenting CI compatibility as support fails the scan.

### PX-E2E-031 — Platform promotion

**Setup:** packaged desktop E2E catalog on macOS; Windows and Linux without packaged E2E.  
**Action:** run the promotion check.  
**Pass:** macOS RELEASE_GRADE with evidence bundle; Windows and Linux remain CI_COMPATIBLE; promotion without platform E2E is rejected.
