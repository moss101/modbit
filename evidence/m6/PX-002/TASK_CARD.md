# Task Card — PX-002 VS Code adapter as a conformant thin client

## Identity

- Task ID: PX-002
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-002; owner label: desktop; subsystem: `packages/vscode-adapter` on `packages/ide-adapter-core`, core (`attention.rs`)
- Qualification: QUAL-PX-002 — a real VS Code extension host loads the adapter against a real local Core: create, steer, approve, review a fixture task from the editor; diagnostics forwarded with revision; restart the editor mid-task and resume by cursor; an adapter attempting to write the workspace, run a tool or read a secret has no code path and the Core rejects any forged command; a revision-mismatched diagnostics batch is discarded with an event.
- Evidence tier: real-system (the real VS Code 1.104.0 extension host through `@vscode/test-electron`; the real Core spawned by the adapter; the built-in TypeScript language service; a scripted OpenAI-compatible server as the model — the live-provider run waits for credentials, DR-M6-001)

## Goal

First IDE adapter built on `packages/ide-adapter-core`: task panel to create, steer, approve and review; forwards language-service diagnostics with revision provenance; shows results and evidence; owns nothing (docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: `packages/vscode-adapter` was an M0 placeholder; the shared client and the conformance contract arrived with PX-001, the diagnostics intake with PX-004.
- production entry points: `packages/vscode-adapter/src/adapter.ts` (`ModbitAdapter`: supervisor as `IDE_ADAPTER`, persisted session and cursor, snapshot + events reduction into the task list, `createTask`, `resume`, `steer`, `approvals` / `decideApproval` with the intent hash shown, `review` / `codeView` / `decideReview` at the revision shown, `forwardDiagnostics` hashing the editor's document text, `attention`), `src/extension.ts` (the `Modbit Tasks` view, the commands, status bar, output channel; `activate` returns the adapter), `package.json` (manifest), `build.mjs`, `test/run.ts` + `test/suite.ts` (extension host), `services/modbit-core/src/attention.rs` (a `RESTART` item for a task that awaited an approval when the Core restarted), `.github/workflows/ci.yml` (both suites in the desktop-e2e job), `docs/decisions/DR-M6-001`.
- proof: in the real extension host the extension activates against the real Core, creates a task from the editor (origin `ide_adapter`) that runs to its protected effect, forwards the built-in TypeScript service's diagnostic — the batch of an unsaved edit is dropped by file revision (`recorded=0`, `discarded≥1`), the reverted document's is recorded at the Core's workspace revision — steers the task, decides the approval by the intent shown (`APPROVED`), opens the review as a document naming the edited file and accepts it with a commit, and the workspace holds exactly what the Core wrote; the adapter's own test restarts the editor mid-task (the tethered Core dies with it), rejoins the same session from the persisted cursor with the task listed `Waiting` from the snapshot, steers, is refused `INTENT_MISMATCH` for another intent, approves, sees the `RESTART` attention item, resumes the suspended run (`resumed=true`), reviews and accepts, and every event it saw is past the cursor it resumed from; PX-001's static scan finds no workspace-write, tool, Git or credential path in the package.

## Limitations

- The live-provider run of PX-E2E-002 (a model choosing its own tools) waits for repository secrets (DR-M6-001); the scripted wire-faithful provider plays the model here.
- The review opens as a read-only markdown document with accept/return commands; per-hunk rejection is the desktop's surface.
- The extension forwards diagnostics on command (`modbit.forwardDiagnostics`), not on every change; the adapter's `forwardDiagnostics` is what an automatic forwarder would call.
- `test:host` downloads a pinned VS Code (1.104.0) on each CI run.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `packages/vscode-adapter/src/adapter.test.ts` ("PX-E2E-002 (harness half): an editor restart mid-task resumes the session by cursor; steer, approve by intent, review")
- `packages/vscode-adapter/test/suite.ts` in the VS Code extension host ("PX-E2E-002: the VS Code adapter drives a real task", five cases)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
