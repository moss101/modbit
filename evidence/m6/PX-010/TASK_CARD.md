# Task Card — PX-010 Issue-to-task intake from CLI and desktop

## Identity

- Task ID: PX-010
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-010; owner label: desktop; subsystem: core (`CreateTask` intake), CLI, desktop composer, shared client
- Qualification: QUAL-PX-010 — from the CLI and the desktop New Task screen create a task from a real issue URL; issue text enters as untrusted context with provenance forge_issue and the task runs the ordinary loop; issue text containing instructions cannot change policy or capabilities; an unreadable issue yields a clear error and no task.
- Evidence tier: real-system (the real Core, CLI and Electron app on one Core; the GitHub fake for the API — the real-issue run waits for a token, DR-M6-002)

## Goal

`modbit task from-issue <url>` and the New Task screen pull a GitHub issue through the forge adapter and create a canonical task with the issue as untrusted context and provenance forge_issue (docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: `CreateTask` accepted origin `forge_issue` as a label; nothing read an issue.
- production entry points: `services/modbit-core/src/server.rs` (`CreateTask` with `issue_url`: the issue read first through `modbit_tools::forge::read_issue`, `FORGE_ISSUE_UNREADABLE` with no task, the goal from the title, `ContextDocumentAttached` + `TaskCreatedFromIssue` in the creation batch), `crates/domain` (`TaskCreatedFromIssue`), `surface.proto` (`CreateTask.issue_url`, `TaskCreated.goal_text`), `apps/cli` (`task from-issue`), `apps/desktop` (the composer's issue field, `task:create` with `issueUrl`), `packages/ide-adapter-core` (`createTask(…, issueUrl)`).
- proof: an unreadable issue (404) and a URL on another host are refused `FORGE_ISSUE_UNREADABLE` naming the cause and no `TaskCreated` lands; a readable issue makes a task named `Totals are wrong for negative quantities (#7)` with origin `forge_issue`, a `ContextDocumentAttached` (trust `UNTRUSTED_EXTERNAL_CONTENT`, source `forge_issue:<url>`) whose text holds the issue's instructions, and `TaskCreatedFromIssue` naming the document; the lease is the profile's default; `context.pack` packs the issue as an attached document labelled data-never-instructions; the ordinary loop runs to review with no approval or attention raised by the text; the desktop's New Task screen and `modbit-cli task from-issue` on the same Core create the same tasks and refuse the same issue.

## Limitations

- the real-issue run waits for a token (DR-M6-002); the issue is read as the Core's own act before the task exists (no tool-call record), then attached under the task; the webhook path is PX-011.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_010_a_task_from_a_forge_issue_carries_the_issue_as_untrusted_context_and_runs_the_ordinary_loop`
- `apps/desktop/e2e/issue-intake.spec.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-34761496840.json` (main at 299f8be; the change commit c2441aa, the DR landing note 76256dd and its manifest reseal 299f8be)
