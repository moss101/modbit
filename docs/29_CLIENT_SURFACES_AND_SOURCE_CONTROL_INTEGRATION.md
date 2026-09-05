# Client surfaces and source-control integration

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 items 4, 5 and 10 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`).  
> **Owners:** desktop (thin clients and their shared library), context-engine (external diagnostics normalization), workspace-git (inline patch, pull requests), external-tools (forge adapter), core-runtime (review-comment steering), verification (CI evidence), sandbox-cloud (webhook intake). No new canonical subsystem.

## One Core, many thin clients

The desktop app, the headless CLI and every external development-environment adapter are **thin clients of the same authenticated SurfaceProtocol** (`30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`). A thin client may:

- submit tasks and steer them: create, steer, pause, resume, cancel, answer questions, approve or deny effects, set execution preferences;
- provide **provenance-bound diagnostics** it already has (an IDE's language service output, a local linter) as evidence attached to a workspace revision;
- display and review results: events by cursor, code review view-models, diffs, artifacts, evidence, release and gate projections.

A thin client never owns orchestration, context selection, memory, Git state, recovery, policy or tool execution, and never holds provider credentials. It cannot bypass approvals, widen a capability, or persist anything the Core does not persist first. Each client authenticates exactly as Electron main does today; a client that cannot obtain the boot-scoped secret or a cloud token gets nothing.

## Thin-client conformance contract (PX-001)

Every client, first-party or adapter, passes the same conformance suite before exposure: command idempotency by `command_id`; cursor replay after disconnect without duplicate commands; approval binding to the intent hash; correct rendering of `Needs Attention` reasons and the acceptance verdict; refusal paths when the Core rejects a command; static proof that the client links no provider, filesystem, Git or policy code path of its own. The CLI is the first client to pass it and is the reference implementation of the contract.

## Clients by release

| Client | Release | Notes |
|---|---|---|
| Desktop app | ALPHA | Electron shell, `32_DESKTOP_FRONTEND_IMPLEMENTATION.md` |
| Headless CLI `modbit` | ALPHA | PX-000; scripts and CI; JSON lines and exit codes |
| VS Code adapter | BETA | PX-002; first IDE adapter, only after the surface protocol is proven by the CLI and desktop in Alpha |
| JetBrains adapter | DEFERRED, POST_ZERO | REQ-PX-003; enters only after the adapter abstraction passes conformance with VS Code; promoted by a later Decision Record |

The shared TypeScript library `packages/ide-adapter-core` implements the contract once; adapters add only IDE-specific hosting of the task panel, diagnostics forwarding and result display. An adapter does not embed or replace the editor's own features; Modbit remains an agent-first workspace, not an IDE.

## Provenance-bound external diagnostics (PX-004)

An adapter may submit diagnostics with `SubmitExternalDiagnostics`: source (IDE and language-service identity and version), workspace revision, file revision, ranges and messages. The Core normalizes them into the same diagnostics records the headless bridge produces (`18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`), tagged `provenance=external_ide`. They are evidence, never authority: they can inform context and verification plans, they cannot satisfy a mandatory verification step on their own, and a revision mismatch discards them. This is also how Tier A language intelligence can be enriched where an IDE is present without making the product depend on one.

## Constrained inline patch (PX-005)

Users may apply a small direct edit from the review surface or a thin client. The only path is the canonical **ChangeTransaction** through the Workspace File Service: revision precondition, path policy after symlink resolution, provenance `user_direct_edit`, one event, one workspace revision advance, stale CodeReferences invalidated. There is no editor buffer model, no unsaved state in any client, and no bypass of protected-path policy. Beta, not Alpha.

## Source-control integration, GitHub first

### Forge adapter (PX-006)

A GitHub adapter lives behind the existing External Tool Hub as the `forge.*` tool family (`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`): `forge.issue.read`, `forge.pr.create`, `forge.pr.update`, `forge.pr.comments.read`, `forge.ci.status`. Calls carry capability leases (`network.egress:api.github.com:443`, `secret.use:github-token`), effect classes (`READ_ONLY` or `EXTERNAL_SIDE_EFFECT`), receipts and idempotency keys. Tokens come from the secret broker; the model never sees them. Other forges implement the same family behind the same hub later.

### Pull request create and update (PX-007)

From the Review screen or the CLI, a reviewed task result can open or update a pull request on a dedicated branch: push and PR creation are protected external effects with approval and receipts, bound to the exact candidate revision and carrying the evidence summary (tests, verification, effects) in the PR body. Updates after revision are new receipts; a stale candidate cannot be pushed.

### Review-comment steering (PX-008)

PR review comments addressed to Modbit become durable `TaskSteered` events with provenance `forge_review_comment` and untrusted-content tagging. Only comments from identities the organization policy allows can steer; a comment can never approve an effect, widen a capability or change policy. The agent treats comment text as data, exactly like repository or web content.

### CI-result ingestion (PX-009)

Check-run and workflow results for the task's branch are ingested as **provenance-bearing verification evidence** artifacts (`ci:<provider>`, run id, commit, logs by OutputRef). They inform the verification plan and appear in Review with their provenance. They are never an automatic qualification PASS: the Verification Engine records them as external evidence, and a qualification that names a real test still requires Modbit's own execution or an approved reproduction policy.

### Issue-to-task intake (PX-010, PX-011)

Locally, `modbit task from-issue <url>` or the desktop New Task screen pulls an issue through `forge.issue.read` and creates a canonical task with the issue as untrusted context and provenance `forge_issue`. When the cloud control plane is present, a GitHub App webhook received by the Cloud API creates the same canonical task for the tenant after signature verification and policy checks; the webhook path adds no second task model. Neither path blocks Alpha.

## Team collaboration (DEFERRED, POST_ZERO)

Shared task history and assignment (REQ-PX-012) and comments with chat notifications (REQ-PX-013) are specified as future work on the cloud projections and desktop surfaces. They are placed in the graph as DEFERRED requirements with qualifications so agents do not build them early, and they do not enter the Release Zero critical path.

## What is out of scope

An embedded editor, Tab completion, or replacing an IDE's own language features remain non-goals. Adapters are entry and review points, not a second product.
