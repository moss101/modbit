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

As built (PX-001): `packages/ide-adapter-core` holds the shared thin-client library — `CoreClient` (the SurfaceProtocol connection: handshake, commands idempotent by the caller's command id, events by cursor, `joinSessionLease` that presents the lease in force rather than fencing out the run this person waits on, `resolveApproval` that always names the intent hash shown) and `CoreSupervisor` (spawns and supervises the profile's Core, with the host's client kind) — which Electron main imports (`apps/desktop/src/main/main.ts`) and an IDE adapter will; and the suite, `runConformance(subject, env)` over a `ConformanceSubject` each client implements on its own protocol path: `idempotency` (the same command id twice is one task, replayed; another id another task), `cursor-replay` (read, drop the connection, resume from the cursor: offsets strictly increasing, nothing repeated, the tail equal to what the Core holds), `attention-rendering` (every `APPROVAL` item of `GetAttention` rendered with its kind, reason and action), `intent-bound-approval` (a decision naming another intent refused `INTENT_MISMATCH` by the Core with the approval still `REQUESTED`; the bound intent `APPROVED`), `verdict-rendering` (the verdict of `GetTaskAssurance.acceptance` rendered verbatim), `rejection-paths` (`UNKNOWN_TASK` / `UNKNOWN_APPROVAL` surfaced with the Core's code and detail, no retry); the suite's truth is its own reference `CoreClient`. The static proof is `scanClientSources` (imports of provider SDKs, Git libraries, policy/tool packages and `node:fs` — sanctioned exceptions named per file with a reason — and a Git or provider process of the client's own) over `packages/ide-adapter-core`, `apps/desktop` and `packages/vscode-adapter`, and `scanCargoClosure` over `cargo metadata` for `modbit-cli` (no `modbit-providers`, `-workspace`, `-git`, `-policy`, `-tools`, `-core-runtime`, `-event-store`, `-terminal`, `-sandbox`, `-effects`, `-secrets` reachable). `Core`: `ResolveApproval.intent_hash` (docs/30). CLI: `task create --command-id <hex>` (prints `replayed`), `approval resolve --intent <hash>`. Test: `packages/ide-adapter-core/src/conformance.test.ts` — the shared client and the CLI pass every case on a real Core in CI's desktop-e2e job; four deliberately broken clients (a fresh id on retry, replay from zero, a decision without the intent, a swallowed refusal) fail at their case; a fixture client linking `simple-git`/`openai`/a credential fails the scan; `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals` proves the Core side of the binding.

## Clients by release

| Client | Release | Notes |
|---|---|---|
| Desktop app | ALPHA | Electron shell, `32_DESKTOP_FRONTEND_IMPLEMENTATION.md` |
| Headless CLI `modbit` | ALPHA | PX-000; scripts and CI; JSON lines and exit codes |
| VS Code adapter | BETA | PX-002; first IDE adapter, only after the surface protocol is proven by the CLI and desktop in Alpha |
| JetBrains adapter | DEFERRED, POST_ZERO | REQ-PX-003; enters only after the adapter abstraction passes conformance with VS Code; promoted by a later Decision Record |

The shared TypeScript library `packages/ide-adapter-core` implements the contract once; adapters add only IDE-specific hosting of the task panel, diagnostics forwarding and result display. An adapter does not embed or replace the editor's own features; Modbit remains an agent-first workspace, not an IDE.

As built (PX-002): `packages/vscode-adapter` — `src/adapter.ts` (`ModbitAdapter`, editor-independent: spawns and supervises the profile's Core through the shared `CoreSupervisor` as `IDE_ADAPTER`, joins the session and cursor the editor persisted so a restarted editor resumes where it was — the snapshot is the state, the events from the cursor are what happened since; `createTask` (origin `ide_adapter`) + `StartTask`, `resume`, `steer` (`QueueInput STEER`), `approvals` / `decideApproval` naming the intent hash shown, `review` / `codeView` / `decideReview` at the revision shown, `forwardDiagnostics` (PX-004: the sha256 of each document's text as the editor has it, bound to the Core's workspace revision), `attention`) and `src/extension.ts` (VS Code hosting only: the `Modbit Tasks` view, commands `modbit.createTask` / `steer` / `approve` / `review` / `decideReview` / `resume` / `forwardDiagnostics`, the status bar, an output channel; `activate` returns the adapter). It reads no workspace file and writes none, runs no tool or Git, holds no credential; the static scan of PX-001 covers it. Tests: `src/adapter.test.ts` (a real Core; the editor "restarts" mid-task — the adapter stops, its tethered Core dies, a new adapter on the same persisted state joins the same session from its cursor with the task still waiting, steers, refuses another intent `INTENT_MISMATCH`, approves, resumes the run the restart suspended, reviews and accepts) and `test/suite.ts` in the real VS Code extension host (`@vscode/test-electron`, VS Code 1.104.0 pinned; the extension creates a task from the editor, the built-in TypeScript service's diagnostic on the fixture is forwarded — an unsaved edit's batch is dropped by revision, the reverted document's is recorded — the approval is decided by its intent, the review is opened and accepted with a commit, and the workspace was written only by the Core). Found on the way: a task that awaited an approval when the Core restarted needs a `StartTask` after the approval is resolved; `GetAttention` now says so (`RESTART`). The live-provider half of PX-E2E-002 waits for credentials (DR-M6-001).

## Provenance-bound external diagnostics (PX-004)

An adapter may submit diagnostics with `SubmitExternalDiagnostics`: source (IDE and language-service identity and version), workspace revision, file revision, ranges and messages. The Core normalizes them into the same diagnostics records the headless bridge produces (`18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`), tagged `provenance=external_ide`. They are evidence, never authority: they can inform context and verification plans, they cannot satisfy a mandatory verification step on their own, and a revision mismatch discards them. This is also how Tier A language intelligence can be enriched where an IDE is present without making the product depend on one.

As built (PX-004): `services/modbit-core/src/external_diagnostics.rs` serves `SubmitExternalDiagnostics` (doc 30). The batch names the task, the adapter (`source`, `source_version`), the workspace revision it saw and, per diagnostic, the root-relative path, a zero-based LSP range, `severity` (`error` | `warning` | `information` | `hint`), an optional code, the message and the sha256 of the file the diagnostic was computed on. A batch for another workspace revision is discarded (`STALE_REVISION`); a malformed one — empty source, unknown severity, empty message, a path that is not root-relative, an inverted range, a file revision that is not a sha256, more than 5000 items — is refused (`MALFORMED`) before anything of it is persisted; both refusals land as `ExternalDiagnosticsRejected` on the task's log. A diagnostic whose file's content is no longer what it was computed on is dropped and counted (`discarded`). What remains is normalized into `modbit_diagnostics::Diagnostic` records (`source` = `external_ide:<adapter>`) inside a batch object (`provenance: external_ide`, the per-file revisions) stored by hash and recorded as `ExternalDiagnosticsRecorded`; a retry with the same command id replays the record. Use: retrieval reads the task's batches at the current revision and links only diagnostics whose file is unchanged (`external_diagnostics` in the planner's request; the hit's reasons carry `diagnostic` and `external_ide`, so a Context Pack entry's `retrieval_reasons` say where the linkage came from and the entry is packed `critical:diagnostic`); the verification plan derived at the run's first write records the batches at its revision under `external_diagnostics` (source, version, revision, batch ref, count, paths) — an input the plan names and nothing more: no command comes from a batch, no check passes because of one, and the repository's mandatory checks run as configured. The shared client exposes `submitExternalDiagnostics` (`packages/ide-adapter-core`). Test: `qual_px_004_external_diagnostics_are_provenance_bound_context_and_never_verification`.

## Constrained inline patch (PX-005)

Users may apply a small direct edit from the review surface or a thin client. The only path is the canonical **ChangeTransaction** through the Workspace File Service: revision precondition, path policy after symlink resolution, provenance `user_direct_edit`, one event, one workspace revision advance, stale CodeReferences invalidated. There is no editor buffer model, no unsaved state in any client, and no bypass of protected-path policy. Beta, not Alpha. As built (PX-005): `ApplyUserPatch` (doc 30; doc 20 "As built (PX-005)"), the desktop review's per-file "Edit" panel and `modbit-cli task patch --path … --revision … --old … --new …`.

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
