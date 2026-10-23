/**
 * Fixtures for the agent list (PX-046) and the conversation rows (PX-047) in
 * the model-free state gallery. Every value is invented for design review:
 * no Core, no model and no task is behind it, and a gallery state is never
 * evidence that a feature works.
 */
import type { AgentHeaderView, StatusClass, TranscriptRowView } from "../../shared/conversation-types.ts";

export const GALLERY_NOW = Date.UTC(2026, 9, 8, 12, 0, 0);

const header = (i: number, statusClass: StatusClass, statusLabel: string, over: Partial<AgentHeaderView> = {}): AgentHeaderView => ({
  taskId: `fixture-task-${i}`, sessionId: "fixture", workspaceRoot: "/work/example", title: `Fixture task ${i}`, subtitle: "example", createdAtMs: GALLERY_NOW - i * 600_000, updatedAtMs: GALLERY_NOW - i * 600_000, statusClass, statusLabel, unread: false, pendingApproval: false, pendingPlan: false, contextPercent: 0, filesChanged: 0, linesAdded: 0, linesRemoved: 0, lastCheckpointAtMs: 0, subagent: false, archived: false, executionLocation: "local", origin: "desktop", taskState: "Running", lastOffset: "1", readOffset: "0", attentionItems: 0, ...over,
});

/** One header of every status class the Core can serve. */
export const AGENT_HEADER_FIXTURES: AgentHeaderView[] = [
  header(1, "NEEDS_ATTENTION", "Needs attention", { title: "Close the stale worktree", pendingApproval: true, attentionItems: 1, taskState: "Waiting" }),
  header(2, "FAILED", "Failed", { title: "Migrate the parser", taskState: "Failed" }),
  header(3, "READY_FOR_REVIEW_UNSEEN", "Ready for review", { title: "Add the export command", unread: true, filesChanged: 3, linesAdded: 42, linesRemoved: 7, taskState: "ReadyForReview" }),
  header(4, "READY_FOR_REVIEW_SEEN", "Ready for review (seen)", { title: "Update the changelog", filesChanged: 1, linesAdded: 4, taskState: "ReadyForReview" }),
  header(5, "RUNNING", "Running", { title: "Fix the flaky parser test" }),
  header(6, "WAITING", "Waiting", { title: "Refactor the loader", taskState: "Waiting" }),
  header(7, "COMPLETED", "Completed", { title: "Rename the config key", taskState: "Completed", origin: "cli" }),
  header(8, "DRAFT", "Draft", { title: "Plan the release notes", taskState: "Queued" }),
  header(9, "ARCHIVED", "Archived", { title: "An old experiment", archived: true, taskState: "Completed" }),
];

const baseHints = { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: "", linesAdded: 0, linesRemoved: 0, status: "" };
const baseRow = { ordinal: 0, offset: "1", lastOffset: "1", atMs: GALLERY_NOW - 60_000, turnId: "turn-1", text: "", textTruncated: false, textRef: "", children: [] as TranscriptRowView[], facts: { type: "none" } as TranscriptRowView["facts"] };
const row = (over: Partial<TranscriptRowView> & Pick<TranscriptRowView, "rowId" | "kind">): TranscriptRowView => ({ ...baseRow, hints: baseHints, ...over });
const stream = (phase: string, abortSource = ""): TranscriptRowView["facts"] => ({ type: "stream", streamId: "fixture-stream", phase, firstOffset: "1", lastOffset: "2", deltaCount: 3, contentHash: "", abortSource, abortCode: abortSource ? "FIXTURE" : "" });

const readTool = row({ rowId: "tool:fixture-read", kind: "TOOL_CARD", text: "fs.read src/lib.rs", hints: { ...baseHints, groupable: true, shortText: "src/lib.rs", status: "SUCCEEDED", durationMs: 40 }, facts: { type: "tool", toolCallId: "t1", toolName: "fs.read", toolClass: "READ", effectClass: "READ", state: "SUCCEEDED", failureCode: "", paths: [] } });
const failedTool = row({ rowId: "tool:fixture-run", kind: "TOOL_CARD", text: "shell.run cargo test", hints: { ...baseHints, groupable: true, shortText: "cargo test", status: "FAILED", durationMs: 3200 }, facts: { type: "tool", toolCallId: "t2", toolName: "shell.run", toolClass: "SHELL", effectClass: "PROCESS", state: "FAILED", failureCode: "NONZERO_EXIT", paths: [] } });

/** A finished turn, a streaming message, an aborted one, an approval and a failed turn: every row kind the surface draws. */
export const CONVERSATION_ROW_FIXTURES: TranscriptRowView[] = [
  row({ rowId: "user:goal", kind: "USER_MESSAGE", text: "Fix the flaky parser test and explain what was wrong.", facts: { type: "user", inputId: "", source: "goal", mode: "", provenance: "", untrusted: false } }),
  row({ rowId: "work:1", kind: "WORK_GROUP", text: "Worked for 12 s", hints: { ...baseHints, durationMs: 12_000 }, facts: { type: "group", turnId: "turn-1", label: "Worked for 12 s", steps: 2, reads: 1, searches: 0, commands: 1, edits: 0, others: 0, linesAdded: 0, linesRemoved: 0, durationMs: 12_000, open: false }, children: [readTool, failedTool] }),
  row({ rowId: "msg:done", kind: "ASSISTANT_MESSAGE", text: "The test raced the file watcher. I made it wait for the event.\n\n```rust\nwatcher.wait();\n```", hints: { ...baseHints, status: "COMPLETE" }, facts: stream("COMPLETED") }),
  row({ rowId: "msg:open", kind: "ASSISTANT_MESSAGE", text: "I am checking the other tests for the same race", hints: { ...baseHints, status: "STREAMING" }, facts: { ...(stream("OPEN") as object), streamId: "fixture-stream-open" } as TranscriptRowView["facts"] }),
  row({ rowId: "msg:aborted", kind: "ASSISTANT_MESSAGE", text: "Partial words before the connection", hints: { ...baseHints, status: "ABORTED", renderable: false }, facts: { ...(stream("ABORTED", "PROVIDER") as object), streamId: "fixture-stream-aborted" } as TranscriptRowView["facts"] }),
  row({ rowId: "unread", kind: "UNREAD_DIVIDER", text: "2 new" }),
  row({ rowId: "approval:1", kind: "APPROVAL_CARD", text: "Run `cargo test -p parser` in the task worktree", hints: { ...baseHints, status: "REQUESTED" }, facts: { type: "approval", kind: "APPROVAL", approvalId: "fixture-approval", toolCallId: "t3", toolName: "shell.run", effectClass: "PROCESS", state: "REQUESTED", resolver: "" } }),
  row({ rowId: "footer:1", kind: "TURN_FOOTER", facts: { type: "footer", turnId: "turn-1", runId: "r1", outcome: "FAILED", failureCode: "PROVIDER_TIMEOUT", durationMs: 14_000, inputTokens: "1200", outputTokens: "340" } }),
];
