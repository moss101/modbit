import { test } from "node:test";
import assert from "node:assert/strict";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import { INITIAL_FOLLOW, callStatus, toolVerb, unfinishedCalls, abortWords, applyStreamEvent, dividerOffsetOf, identityOf, messageIds, needsEyes, onDismiss, onJumpToBottom, onMessages, onScroll, pillText, presentAssistant, reuseRows, seedStreams, stallLevel, withDivider, withLiveOnly, elapsedWords, isPlanRow, planProgress, planSteps, tokenWords, turnStartMs, withPlanCard, type LiveStreams } from "./model.ts";
import { sameBlock, splitBlocks } from "./blocks.ts";
import { DEFAULT_CONVERSATION_PREFS, parseConversationPrefs } from "./prefs.ts";

const SID = "0123456789abcdef0123456789abcdef";
const HYPHEN = "01234567-89ab-cdef-0123-456789abcdef";

function row(over: Partial<TranscriptRowView> & { kind: TranscriptRowView["kind"] }): TranscriptRowView {
  return { ordinal: 1, rowId: "r", offset: "10", lastOffset: "10", atMs: 0, turnId: "", hints: { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: "", linesAdded: 0, linesRemoved: 0, status: "" }, text: "", textTruncated: false, textRef: "", children: [], facts: { type: "none" }, ...over };
}
const streamRow = (phase: string, text = "", abort = "RECOVERY"): TranscriptRowView =>
  row({ kind: "ASSISTANT_MESSAGE", rowId: `msg:${HYPHEN}`, text, facts: { type: "stream", streamId: HYPHEN, phase, firstOffset: "5", lastOffset: "9", deltaCount: 1, contentHash: "", abortSource: phase === "ABORTED" ? abort : "", abortCode: phase === "ABORTED" ? "CORE_RESTART" : "" } });
const ev = (eventType: string, payload: unknown, taskId: string | null = "t1") => ({ eventType, aggregateType: "assistant_stream", aggregateId: SID, taskId, occurredAtMs: 1000, payload });
const delta = (sequence: number, text: string, kind = "TEXT") => ev("AssistantTextDelta", { kind, sequence, text });
const apply = (live: LiveStreams, e: ReturnType<typeof ev>) => applyStreamEvent(live, e, "t1");

test("deltas build the text in order; a redelivery adds nothing; a gap waits for the missing piece", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(1, "Hello "));
  live = apply(live, delta(3, "world"));
  assert.equal(live.get(SID)!.text, "Hello ", "a piece ahead of a gap is held, not appended");
  live = apply(live, delta(2, "brave "));
  assert.equal(live.get(SID)!.text, "Hello brave world");
  const same = apply(live, delta(2, "brave "));
  assert.equal(same, live, "a redelivered piece changes nothing");
});

test("reasoning never joins the answer text; another task's stream is ignored", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(1, "thinking", "REASONING"));
  assert.equal(live.size, 0);
  assert.equal(applyStreamEvent(live, delta(1, "x"), "other-task"), live);
});

test("a partial stream is never shown as final: open streams stream, aborted ones are aborted with their cause, only completed ones are complete", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(1, "partial answer"));
  const open = presentAssistant(streamRow("OPEN"), live);
  assert.equal(open.message, "streaming");
  assert.equal(open.text, "partial answer");
  // The abort arrives before the projection is re-read: still aborted, never complete.
  live = apply(live, ev("AssistantMessageAborted", { source: "RECOVERY", code: "CORE_RESTART" }));
  const aborted = presentAssistant(streamRow("OPEN"), live);
  assert.equal(aborted.message, "aborted");
  assert.match(aborted.abort!, /restarted/);
  const fromCore = presentAssistant(streamRow("ABORTED", "partial answer"), new Map());
  assert.equal(fromCore.message, "aborted");
  assert.equal(presentAssistant(streamRow("COMPLETED", "done text"), new Map()).message, "complete");
  assert.match(abortWords("USER_INTERRUPT", ""), /interrupted/);
  assert.match(abortWords("PROVIDER", "TIMEOUT"), /TIMEOUT/);
});

test("a streaming row only grows: a shorter projection never replaces longer live text", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(1, "0123456789"));
  assert.equal(presentAssistant(streamRow("OPEN", "0123"), live).text, "0123456789");
  assert.equal(presentAssistant(streamRow("OPEN", "0123456789abc"), live).text, "0123456789abc");
});

test("seeding from main resumes a stream after a reload and deduplicates by sequence", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(4, "d"));
  live = seedStreams(live, [{ streamId: SID, taskId: "t1", sequence: 3, text: "abc" }], 5);
  assert.equal(live.get(SID)!.text, "abcd", "the held piece joins the seeded text");
  const again = seedStreams(live, [{ streamId: SID, taskId: "t1", sequence: 3, text: "abc" }], 6);
  assert.equal(again, live);
});

test("a live stream with no projected row yet is shown as one message, keyed like its row", () => {
  let live: LiveStreams = new Map();
  live = apply(live, delta(1, "first words"));
  const body = withLiveOnly([], live);
  assert.equal(body.length, 1);
  assert.equal(identityOf(body[0]!), identityOf(streamRow("OPEN")), "one identity from first piece to projected row: it is counted once");
  assert.equal(withLiveOnly([streamRow("OPEN")], live).length, 1, "once the projection has the row nothing is added");
});

test("an approval, a failure, an abort or a failed turn inside a fold keeps the fold open", () => {
  const group = (child: TranscriptRowView) => row({ kind: "WORK_GROUP", rowId: "g", children: [child] });
  const ok = row({ kind: "TOOL_CARD", hints: { ...row({ kind: "TOOL_CARD" }).hints, status: "SUCCEEDED" } });
  assert.equal(needsEyes(group(ok)), false);
  assert.equal(needsEyes(group(row({ kind: "TOOL_CARD", hints: { ...ok.hints, status: "FAILED" } }))), true);
  assert.equal(needsEyes(group(row({ kind: "APPROVAL_CARD", hints: { ...ok.hints, status: "REQUESTED" } }))), true);
  assert.equal(needsEyes(group(streamRow("ABORTED"))), true);
  assert.equal(needsEyes(group(row({ kind: "TURN_FOOTER", facts: { type: "footer", turnId: "", runId: "", outcome: "FAILED", failureCode: "X", durationMs: 0, inputTokens: "0", outputTokens: "0" } }))), true);
  assert.equal(needsEyes(group(row({ kind: "APPROVAL_CARD", hints: { ...ok.hints, status: "APPROVED" } }))), false);
});

test("follow: scrolling up freezes it, messages are counted only then, dismiss hides until more arrive, bottom resets", () => {
  let f = INITIAL_FOLLOW;
  f = onMessages(f, 2);
  assert.equal(f.unseen, 0, "pinned: nothing to count");
  f = onScroll(f, 400);
  assert.equal(f.pinned, false);
  f = onMessages(f, 2);
  f = onMessages(f, 1);
  assert.equal(f.unseen, 3);
  assert.equal(pillText(3), "3 new messages");
  assert.equal(pillText(1), "1 new message");
  f = onDismiss(f);
  assert.equal(f.dismissed, true);
  f = onMessages(f, 1);
  assert.equal(f.dismissed, false);
  assert.equal(f.unseen, 4);
  assert.deepEqual(onScroll(f, 10), INITIAL_FOLLOW, "back within the slack of the bottom follows again");
  assert.deepEqual(onJumpToBottom(), INITIAL_FOLLOW);
});

test("the stall ladder escalates the wording at 2, 8 and 32 seconds and always names the action", () => {
  assert.equal(stallLevel("Running a command: npm test", 100).level, 0);
  assert.equal(stallLevel("Running a command: npm test", 2500).level, 1);
  assert.equal(stallLevel("Running a command: npm test", 9000).level, 2);
  const stuck = stallLevel("Running a command: npm test", 40_000);
  assert.equal(stuck.level, 3);
  for (const ms of [0, 3000, 9000, 40_000]) assert.match(stallLevel("Running a command: npm test", ms).text, /Running a command: npm test/);
  assert.match(stallLevel("", 0).text, /Working/);
});

test("the unread divider survives the person reading: it is re-inserted before the first unread row", () => {
  const rows = [row({ kind: "USER_MESSAGE", rowId: "u", offset: "5" }), row({ kind: "ASSISTANT_MESSAGE", rowId: "a", offset: "12" }), row({ kind: "ASSISTANT_MESSAGE", rowId: "b", offset: "15" })];
  assert.equal(dividerOffsetOf(rows), null);
  const withCore = [rows[0]!, row({ kind: "UNREAD_DIVIDER", rowId: "unread", offset: "12", text: "2 new" }), rows[1]!, rows[2]!];
  assert.equal(dividerOffsetOf(withCore), "12");
  const rebuilt = withDivider(rows, "12", (n) => `${n} new`);
  assert.deepEqual(rebuilt.map((r) => r.kind), ["USER_MESSAGE", "UNREAD_DIVIDER", "ASSISTANT_MESSAGE", "ASSISTANT_MESSAGE"]);
  assert.equal(rebuilt[1]!.text, "2 new");
  assert.equal(withDivider(withCore, "12", (n) => `${n}`).length, 4, "the Core's own divider is not doubled");
  assert.equal(withDivider(rows, "5", (n) => `${n}`).length, 3, "nothing before the first row: no divider");
  assert.deepEqual(messageIds(rows), ["u", "a", "b"]);
});

test("a re-read reuses the unchanged row objects, so memoised rows skip them", () => {
  const a = row({ kind: "USER_MESSAGE", rowId: "u", text: "hi" });
  const b = row({ kind: "ASSISTANT_MESSAGE", rowId: "a", text: "x", lastOffset: "10" });
  const next = reuseRows([a, b], [{ ...a }, { ...b, lastOffset: "11", text: "xy" }]);
  assert.equal(next[0], a);
  assert.notEqual(next[1], b);
  assert.equal(next[1]!.text, "xy");
});

test("blocks: paragraphs and fenced code; an unclosed fence is an open code block; a delta changes only the last block", () => {
  const b = splitBlocks("First para\nstill first\n\nSecond para\n\n```ts\nconst x = 1;\n```\nafter");
  assert.deepEqual(b.map((x) => x.kind), ["text", "text", "code", "text"]);
  assert.equal(b[2]!.kind === "code" && b[2]!.lang, "ts");
  const open = splitBlocks("intro\n\n```\nlet y");
  assert.equal(open[1]!.kind === "code" && open[1]!.open, true);
  const before = splitBlocks("one\n\ntwo wor");
  const after = splitBlocks("one\n\ntwo words");
  assert.equal(sameBlock(before[0]!, after[0]!), true);
  assert.equal(sameBlock(before[1]!, after[1]!), false);
  // Markup is just text: nothing is parsed out of it.
  assert.deepEqual(splitBlocks("<img src=x onerror=alert(1)>"), [{ kind: "text", text: "<img src=x onerror=alert(1)>" }]);
});

test("conversation preferences default to Compact with code folded, and reject hostile values", () => {
  assert.deepEqual(parseConversationPrefs(null), DEFAULT_CONVERSATION_PREFS);
  assert.equal(DEFAULT_CONVERSATION_PREFS.density, "COMPACT");
  assert.deepEqual(parseConversationPrefs('{"density":"DETAILED","codeOpen":true}'), { density: "DETAILED", codeOpen: true });
  assert.deepEqual(parseConversationPrefs('{"density":"<b>","codeOpen":1}'), DEFAULT_CONVERSATION_PREFS);
  assert.deepEqual(parseConversationPrefs("[1]"), DEFAULT_CONVERSATION_PREFS);
});

test("a step is in the past tense only once it happened", () => {
  assert.equal(toolVerb("SHELL", "SUCCEEDED"), "Ran");
  assert.equal(toolVerb("SHELL", "RUNNING"), "Running");
  for (const status of ["PENDING", "AWAITING_APPROVAL", "DENIED", "CANCELLED", "FAILED", "UNKNOWN_OUTCOME"]) assert.equal(toolVerb("SHELL", status), "Run", status);
  assert.equal(toolVerb("EDIT", "SUCCEEDED"), "Edited");
  assert.equal(toolVerb("EDIT", "AWAITING_APPROVAL"), "Edit");
  assert.equal(toolVerb("NO_SUCH_CLASS", "SUCCEEDED"), "Used");
});

test("a fold says which of its calls did not finish, failed, ended unknown or did not run", () => {
  assert.deepEqual(unfinishedCalls(["SUCCEEDED", "FAILED"]), ["1 failed"]);
  assert.deepEqual(unfinishedCalls(["UNKNOWN_OUTCOME", "SUCCEEDED"]), ["1 with an unknown outcome"]);
  assert.deepEqual(unfinishedCalls(["SUCCEEDED", "AWAITING_APPROVAL", "RUNNING"]), ["2 not finished"]);
  assert.deepEqual(unfinishedCalls(["DENIED", "CANCELLED", "PENDING"]), ["1 not finished", "2 not run"]);
  // A call whose status is not one of the named ones (no hint yet, a state between dispatch and the first fact) is
  // still told, as not finished: a fold never loses the words for a call (the AWL gate "step does not say what it did").
  assert.deepEqual(unfinishedCalls([""]), ["1 not finished"]);
  assert.deepEqual(unfinishedCalls(["DISPATCHED", "SUCCEEDED"]), ["1 not finished"]);
});

test("a call denied before it ran is told as denied, never as a failed command", () => {
  assert.equal(callStatus("FAILED", "APPROVAL_DENIED"), "DENIED");
  assert.equal(callStatus("FAILED", "POLICY_DENIED"), "DENIED");
  assert.equal(callStatus("FAILED", "NON_ZERO_EXIT"), "FAILED");
  assert.equal(callStatus("SUCCEEDED", ""), "SUCCEEDED");
});

// ------------------------------------------------------------------ PX-200

const tool = (rowId: string, toolName: string, over: Partial<TranscriptRowView> = {}): TranscriptRowView => row({ kind: "TOOL_CARD", rowId, facts: { type: "tool", toolCallId: "", toolName, toolClass: toolName.startsWith("fs.") ? "READ" : "OTHER", effectClass: "", state: "SUCCEEDED", failureCode: "", paths: [] }, ...over });
const group = (rowId: string, children: TranscriptRowView[]): TranscriptRowView => row({ kind: "WORK_GROUP", rowId, children, facts: { type: "group", turnId: "", label: "", steps: children.length, reads: 0, searches: 0, commands: 0, edits: 0, others: children.length, linesAdded: 0, linesRemoved: 0, durationMs: 0, open: false } });

test("PX-200: with steps in the work graph one card stands where the latest plan version was recorded, and every plan step leaves the transcript", () => {
  const user = row({ kind: "USER_MESSAGE", rowId: "u" });
  const said = row({ kind: "ASSISTANT_MESSAGE", rowId: "m1" });
  const rows = [user, tool("plan:1", "plan.update"), group("g1", [tool("t1", "fs.read"), tool("plan:2", "plan.update")]), said, group("g2", [tool("plan:3", "plan.update")]), tool("t2", "fs.read"), row({ kind: "ASSISTANT_MESSAGE", rowId: "m2" })];
  assert.equal(isPlanRow(rows[1]!), true);
  assert.equal(isPlanRow(rows[5]!), false);
  const placed = withPlanCard(rows, true);
  assert.deepEqual(placed.rows.map((r) => r.rowId), ["u", "g1", "m1", "t2", "m2"], "plan steps are gone, alone or in a fold, and a fold of only plan steps with them");
  assert.equal(placed.cardAt, 3, "the card follows what stood before the latest plan version");
  const g1 = placed.rows[1]!;
  assert.deepEqual(g1.children.map((c) => c.rowId), ["t1"]);
  assert.deepEqual(g1.facts.type === "group" ? [g1.facts.steps, g1.facts.others] : [], [1, 1], "the fold counts what it still holds");
  assert.equal(placed.rows[0], user, "a row that holds no plan step is the same object, so its memoised view is kept");
  assert.equal(placed.rows[3], rows[5]);
  // No steps in the graph: nothing is taken out and no card is drawn (the plan line stays).
  const kept = withPlanCard(rows, false);
  assert.equal(kept.rows, rows);
  assert.equal(kept.cardAt, null);
  // Steps in the graph and no plan step on the page: the card stands at the end.
  assert.deepEqual(withPlanCard([user, said], true), { rows: [user, said], cardAt: 2 });
  // The plan was the first thing: the card is first.
  assert.equal(withPlanCard([tool("plan:1", "plan.update"), said], true).cardAt, 0);
});

test("PX-200: a step's state is the Core's; an active step spins only while the task runs", () => {
  const steps = [
    { id: "a", title: "A", status: "DONE", blockers: [] },
    { id: "b", title: "B", status: "ACTIVE", blockers: [] },
    { id: "c", title: "C", status: "BLOCKED", blockers: ["needs a key", " "] },
    { id: "d", title: "D", status: "FAILED", blockers: [] },
    { id: "e", title: "E", status: "CANCELLED", blockers: [] },
    { id: "f", title: "F", status: "READY", blockers: [] },
    { id: "g", title: "G", status: "PENDING", blockers: [] },
    { id: "h", title: "H", status: "SOMETHING_NEW", blockers: [] },
  ];
  const running = planSteps(steps, { running: true, needsPerson: false });
  assert.deepEqual(running.map((s) => [s.tone, s.words, s.current]), [["ok", "Done", false], ["running", "In progress", true], ["warn", "Blocked", false], ["danger", "Failed", false], ["idle", "Dropped", false], ["idle", "Next", false], ["idle", "Not started", false], ["idle", "something new", false]]);
  assert.deepEqual(running[2]!.blockers, ["needs a key"]);
  assert.deepEqual(planSteps(steps, { running: false, needsPerson: true })[1], { id: "b", title: "B", status: "ACTIVE", tone: "warn", words: "In progress, waiting for you", blockers: [], current: true });
  assert.equal(planSteps(steps, { running: false, needsPerson: false })[1]!.words, "In progress, not running now");
  assert.equal(planSteps(steps, { running: false, needsPerson: true })[0]!.words, "Done", "only an active step is touched");
  assert.equal(planProgress(steps), "1 of 7 done, 1 failed, 1 blocked, 1 dropped");
  assert.equal(planProgress([{ status: "DONE" }, { status: "DONE" }]), "2 of 2 done");
});

test("PX-200: the status line's clock and token count", () => {
  assert.deepEqual([0, 999, 42_000, 60_000, 192_000, 3_600_000 + 4 * 60_000, -5].map(elapsedWords), ["0s", "0s", "42s", "1m 00s", "3m 12s", "1h 04m", "0s"]);
  assert.deepEqual([0n, 1n, 850n, 1000n, 18_449n, 999_999n, 1_250_000n].map(tokenWords), ["", "1 token", "850 tokens", "1.0k tokens", "18.4k tokens", "999.9k tokens", "1.2M tokens"]);
  const u1 = row({ kind: "USER_MESSAGE", rowId: "u1", atMs: 1000 });
  const work = tool("t1", "fs.read", { atMs: 2000 });
  const queued = row({ kind: "USER_MESSAGE", rowId: "u2", atMs: 9000 });
  assert.equal(turnStartMs([u1, work]), 1000);
  assert.equal(turnStartMs([u1, work, queued]), 1000, "a message that is only queued does not restart the clock");
  assert.equal(turnStartMs([u1, work, queued, tool("t2", "fs.read", { atMs: 9500 })]), 9000, "once work follows it, it is the turn's start");
  assert.equal(turnStartMs([work]), 2000, "no message on the page: the first thing with a time");
  assert.equal(turnStartMs([]), 0);
});
