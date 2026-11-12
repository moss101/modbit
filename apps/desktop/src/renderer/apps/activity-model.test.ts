import { test } from "node:test";
import assert from "node:assert/strict";
import type { RowFacts, RowKind, TranscriptRowView } from "../../shared/conversation-types.ts";
import { activityOf, clock, eventTitle, filterActivity, kindCounts } from "./activity-model.ts";
import type { DeliveredEvent } from "./delivered.ts";

const hints = { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: "", linesAdded: 0, linesRemoved: 0, status: "SUCCEEDED" };
const row = (rowId: string, kind: RowKind, offset: number, over: Partial<TranscriptRowView> = {}): TranscriptRowView => ({ ordinal: 0, rowId, kind, offset: String(offset), lastOffset: String(offset), atMs: 1_700_000_000_000 + offset, turnId: "t", hints, text: "", textTruncated: false, textRef: "", children: [], facts: { type: "none" }, ...over });
const tool = (toolClass: string, toolName: string, over: Partial<Extract<RowFacts, { type: "tool" }>> = {}): RowFacts => ({ type: "tool", toolCallId: "c", toolName, toolClass, effectClass: "", state: "", failureCode: "", paths: [], ...over });
const event = (offset: number, eventType: string, aggregateType = "run", payload: unknown = {}): DeliveredEvent => ({ offset: String(offset), eventType, aggregateType, occurredAtMs: 1_700_000_000_000 + offset, payload });

const ROWS: TranscriptRowView[] = [
  row("user:1", "USER_MESSAGE", 10, { text: "Tidy the notes\nand more" }),
  row("group:1", "WORK_GROUP", 12, {
    lastOffset: "30",
    facts: { type: "group", turnId: "t", label: "Worked", steps: 3, reads: 1, searches: 0, commands: 1, edits: 1, others: 0, linesAdded: 1, linesRemoved: 0, durationMs: 5, open: false },
    children: [
      row("tool:read", "TOOL_CARD", 12, { lastOffset: "14", text: "notes.txt", facts: tool("READ", "fs.read") }),
      row("tool:edit", "TOOL_CARD", 16, { lastOffset: "20", text: "notes.txt", facts: tool("EDIT", "change.apply", { paths: ["notes.txt"] }) }),
      row("tool:run", "TOOL_CARD", 22, { lastOffset: "30", text: "cargo test", hints: { ...hints, status: "FAILED" }, facts: tool("SHELL", "shell.run", { failureCode: "EXIT_1" }) }),
    ],
  }),
  row("approval:1", "APPROVAL_CARD", 32, { lastOffset: "33", facts: { type: "approval", kind: "TOOL", approvalId: "a", toolCallId: "c", toolName: "git.push", effectClass: "External", state: "PENDING", resolver: "" } }),
  row("msg:1", "ASSISTANT_MESSAGE", 40, { lastOffset: "44", text: "\nDone: the notes are tidy.\nMore." }),
  row("footer:1", "TURN_FOOTER", 46, { facts: { type: "footer", turnId: "t", runId: "r", outcome: "COMPLETED", failureCode: "", durationMs: 9, inputTokens: "1", outputTokens: "1" } }),
  row("tail", "TAIL_STATUS", 47, { facts: { type: "tail", phase: "idle", label: "Idle", detail: "" } }),
  row("gap", "TIME_BOUNDARY", 48, { facts: { type: "boundary", gapMs: 1, day: "" } }),
];

test("REQ-PX-202: Activity lists the delivered transcript in the log's order, each step worded from its own facts", () => {
  const list = activityOf(ROWS, []);
  assert.deepEqual(list.map((e) => `${e.offset} ${e.kind} ${e.text}`), [
    "10 message You: Tidy the notes",
    "12 read Read notes.txt",
    "16 edit Edited notes.txt",
    "22 command Run cargo test (failed)",
    "32 approval Asked to push commits (git.push)",
    "40 message Agent: Done: the notes are tidy.",
    "46 turn Turn finished",
  ]);
  assert.ok(list.every((e) => e.source === "transcript"));
  assert.equal(list[3]!.tone, "bad");
  assert.equal(list[4]!.tone, "ask");
  assert.deepEqual(list.map((e) => e.key), ["user:1", "tool:read", "tool:edit", "tool:run", "approval:1", "msg:1", "footer:1"], "the conversation's furniture (tail, time boundary) is not activity");
  assert.deepEqual(kindCounts(list), { message: 2, read: 1, edit: 1, command: 1, approval: 1, turn: 1 });
});

test("REQ-PX-202: a group the Core delivered folded is one entry; nothing is invented for its steps", () => {
  const folded = [row("group:1", "WORK_GROUP", 12, { lastOffset: "30", facts: { type: "group", turnId: "t", label: "Read a file, edited a file", steps: 2, reads: 1, searches: 0, commands: 0, edits: 1, others: 0, linesAdded: 1, linesRemoved: 0, durationMs: 5, open: false } })];
  assert.deepEqual(activityOf(folded, []).map((e) => e.text), ["Read a file, edited a file"]);
});

test("REQ-PX-202: a delivered event the transcript already tells is not listed twice; one it does not tell takes its place in the order", () => {
  const events = [event(13, "ToolCallStarted"), event(33, "ApprovalRequested", "approval"), event(35, "CheckpointCommitted", "checkpoint"), event(50, "VerificationRunRecorded", "run", { stage: "targeted", status: "PASSED" }), event(52, "TaskReadyForReview", "task")];
  const list = activityOf(ROWS, events);
  assert.deepEqual(list.filter((e) => e.source === "event").map((e) => `${e.offset} ${e.kind} ${e.text}`), ["35 other Checkpoint committed", "50 check Verification run recorded: targeted, passed", "52 other Task ready for review"], "13 lies inside the read step and 33 inside the approval's row: the rows already tell them");
  assert.deepEqual(list.map((e) => e.offset), ["10", "12", "16", "22", "32", "35", "40", "46", "50", "52"]);
  assert.equal(list.find((e) => e.offset === "50")!.tone, "ok");
  // With no transcript delivered (the conversation was never opened) the events alone are the list.
  assert.deepEqual(activityOf([], events).map((e) => e.offset), ["13", "33", "35", "50", "52"]);
  assert.equal(activityOf([], events)[1]!.kind, "approval");
});

test("REQ-PX-202: filter by kind and find text; a query is every word, in any case", () => {
  const list = activityOf(ROWS, [event(50, "VerificationRunRecorded", "run", { stage: "targeted", status: "PASSED" })]);
  assert.deepEqual(filterActivity(list, "edit", "").map((e) => e.key), ["tool:edit"]);
  assert.deepEqual(filterActivity(list, "all", "NOTES").map((e) => e.key), ["user:1", "tool:read", "tool:edit", "msg:1"]);
  assert.deepEqual(filterActivity(list, "all", "notes tidy").map((e) => e.key), ["user:1", "msg:1"]);
  assert.deepEqual(filterActivity(list, "message", "tidy the").map((e) => e.key), ["user:1", "msg:1"]);
  assert.deepEqual(filterActivity(list, "check", "").map((e) => e.key), ["event:50"]);
  assert.deepEqual(filterActivity(list, "all", "no such words"), []);
  assert.equal(filterActivity(list, "all", "   ").length, list.length);
});

test("REQ-PX-202: an event is worded from its own type and a few named fields, never from the rest of its payload", () => {
  assert.equal(eventTitle("VerificationRunRecorded"), "Verification run recorded");
  assert.equal(eventTitle("TaskReadyForReview"), "Task ready for review");
  const [e] = activityOf([], [event(1, "AcceptanceGateEvaluated", "run", { verdict: "ACCEPT", secret: "sk-live-1", nested: { status: "x" }, reason: "x".repeat(200) })]);
  assert.equal(e!.text, "Acceptance gate evaluated: accept");
  assert.equal(clock(0), "");
  assert.match(clock(1_700_000_000_000), /^\d\d:\d\d:\d\d$/);
});
