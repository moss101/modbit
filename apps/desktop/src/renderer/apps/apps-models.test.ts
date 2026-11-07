import assert from "node:assert/strict";
import { test } from "node:test";
import { callRowId, canCommitPush, canDecide, changeSummary, chipText, commitLabel, decisionWords, commitMode, effectWords, isReviewable, lineKind, newCommandId, rangesContain, refusalWords, stageState } from "./changes-model.ts";
import { categoryLabel, percent, repairLine, repairSummary, windowLine } from "./evidence-model.ts";
import { formatBytes, parentOf, refusalText, withheldText } from "./files-model.ts";
import { groupByTask } from "./use-terminals.ts";

test("PX-048: only a task that is ready for review takes a decision; the diff shows in every state", () => {
  assert.equal(isReviewable("ReadyForReview"), true);
  for (const s of ["Running", "Waiting", "Failed", "Completed", "Queued"]) assert.equal(isReviewable(s), false, s);
  assert.equal(lineKind("+x"), "add");
  assert.equal(lineKind("-x"), "del");
  assert.equal(lineKind(" x"), "ctx");
  assert.equal(changeSummary([]), "No changes");
  assert.equal(changeSummary([{ hunks: [{}, {}], binary: false }]), "1 file, 2 hunks");
  assert.equal(changeSummary([{ hunks: [{}], binary: false }, { hunks: [], binary: true }]), "2 files, 1 hunk");
});

test("PX-048: the Core's changed ranges are inclusive 1-based start,end pairs", () => {
  const r = [2, 3, 10, 10];
  assert.deepEqual([1, 2, 3, 4, 9, 10, 11].map((l) => rangesContain(r, l)), [false, true, true, false, false, true, false]);
  assert.equal(rangesContain([], 1), false);
  assert.equal(rangesContain([5], 5), false, "a dangling start is not a range");
});

test("PX-048: Files shows the Core's typed refusals in plain words and never a blank pane", () => {
  assert.match(refusalText("OUTSIDE_ROOT: path `../x` resolves outside the workspace root"), /outside the task's workspace/);
  assert.match(refusalText("PROTECTED: path `.env` is protected"), /protected/);
  assert.match(refusalText("NOT_FOUND: nope"), /no longer exists/);
  assert.match(refusalText("BAD_ARGUMENT: path must be workspace-relative"), /not a workspace-relative path/);
  assert.match(refusalText("something odd"), /could not be read: something odd/);
  assert.match(withheldText("BINARY", "2048") ?? "", /Binary file \(2\.0 KiB\)/);
  assert.match(withheldText("TOO_LARGE", String(300 * 1024)) ?? "", /Too large to show inline \(300\.0 KiB\)/);
  assert.equal(withheldText("TEXT", "10"), null);
  assert.equal(formatBytes(12), "12 B");
  assert.equal(formatBytes(5 * 1024 * 1024), "5.0 MiB");
  assert.equal(parentOf("a/b/c.rs"), "a/b");
  assert.equal(parentOf("c.rs"), "");
});

test("PX-048/PX-059: the Evidence app states the Core's context numbers as the Core did, including an unknown window", () => {
  assert.equal(percent(1250), "12.5%");
  assert.equal(categoryLabel("tool_definitions"), "Tool definitions");
  assert.equal(windowLine({ totalTokens: "1200", totalSource: "PROVIDER_REPORTED", windowTokens: "128000", windowSource: "REGISTRY", usedBp: 94, model: "gpt-x" }), "1200 tokens (reported by the provider) of a 128000-token window (0.9%) for gpt-x.");
  assert.match(windowLine({ totalTokens: "900", totalSource: "ESTIMATED", windowTokens: "0", windowSource: "UNKNOWN", usedBp: 0, model: "" }), /900 tokens \(estimated\); the model's context window is not known\./);
});

test("PX-048: the terminal list groups by owning task and drops host-owned terminals without a task", () => {
  const t = (terminalId: string, taskId: string) => ({ terminalId, taskId }) as never;
  const g = groupByTask([t("1", "a"), t("2", "b"), t("3", "a"), t("4", "")]);
  assert.deepEqual([...g.keys()], ["a", "b"]);
  assert.equal(g.get("a")?.length, 2);
});


test("changes actions: a path is staged, partly staged or not staged, from the Core's two columns", () => {
  assert.equal(stageState({ staged: "", unstaged: "M" }), "none");
  assert.equal(stageState({ staged: "M", unstaged: "" }), "staged");
  assert.equal(stageState({ staged: "A", unstaged: "M" }), "partial");
});

test("changes actions: the toolbar commits staged files, or only pushes commits the remote lacks", () => {
  assert.equal(commitMode(2, 0), "commit");
  assert.equal(commitMode(1, 3), "commit");
  assert.equal(commitMode(0, 3), "push");
  assert.equal(commitMode(0, 0), "none");
  assert.equal(commitLabel("push", 1), "Push 1 commit");
  assert.equal(commitLabel("push", 2), "Push 2 commits");
  assert.equal(commitLabel("commit", 0), "Commit and push");
  assert.equal(canCommitPush("commit", "  ", false), false, "a commit needs a message");
  assert.equal(canCommitPush("commit", "Fix it", false), true);
  assert.equal(canCommitPush("push", "", false), true, "a push alone needs none");
  assert.equal(canCommitPush("none", "x", false), false);
  assert.equal(canCommitPush("commit", "x", true), false, "not while the task runs");
});

test("changes actions: every typed refusal has words, and an unknown code still says something", () => {
  for (const code of ["STALE_REVISION", "DIRTY_CONFLICT", "PROTECTED_PATH", "NO_REMOTE", "NOTHING_STAGED", "STALE_INTENT", "PUSH_REJECTED", "TASK_RUNNING"]) assert.ok(refusalWords(code).length > 10, code);
  assert.match(refusalWords("SOMETHING_NEW"), /refused/);
  assert.match(refusalWords(""), /could not/);
});

test("changes actions: an authorization with no result is shown as in doubt, never as done", () => {
  assert.match(effectWords({ state: "AUTHORIZED_IN_DOUBT", commit: "", code: "" }), /unknown/);
  assert.match(effectWords({ state: "COMPLETED", commit: "0123456789abcdef", code: "" }), /0123456789 and pushed/);
  assert.match(effectWords({ state: "FAILED", commit: "", code: "PUSH_REJECTED" }), /PUSH_REJECTED/);
});

test("changes actions: a command id is 32 hex characters and differs per click", () => {
  const a = newCommandId();
  const b = newCommandId();
  assert.match(a, /^[0-9a-f]{32}$/);
  assert.notEqual(a, b);
});

test("PX-104: the chip says what the Core attributed and a decision never waits for the task to stop", () => {
  assert.equal(chipText({ attributed: true, toolName: "change.apply", toolCallId: "c1" }), "Written by change.apply");
  assert.equal(chipText({ attributed: false, toolName: "", toolCallId: "" }), "Not attributed to a tool call");
  assert.match(chipText({ attributed: false, toolName: "shell.exec", toolCallId: "c2" }), /Partly written by shell.exec/);
  assert.equal(callRowId("0192-abc"), "tool:0192-abc");
  assert.equal(canDecide(false, false), true, "a running task does not disable a decision");
  assert.equal(canDecide(true, false), false);
  assert.equal(canDecide(false, true), false, "a binary file reverts as a whole file");
  assert.match(decisionWords({ result: "REJECTED", path: "a.txt", hunkIndex: 1, runLive: true, agentNotice: "n" }), /Rejected hunk 2 of a.txt; the run goes on/);
  assert.equal(decisionWords({ result: "ACCEPTED", path: "a.txt", hunkIndex: 0, runLive: false, agentNotice: "" }), "Accepted hunk 1 of a.txt.");
  assert.match(refusalWords("STALE_HUNK"), /no longer the one you were shown/);
  assert.match(refusalWords("UNATTRIBUTED_HUNK"), /never accepted automatically/);
});

test("DR-PX-2026-10-25-017: a repair attempt is shown as what was tried and what the Core recorded of it, an escalation as a stop", () => {
  const open = { ordinal: 1, failureSignature: "ts:2322", hypothesis: "the argument is a string", intendedFix: "src/a.ts", outcome: "", reverted: false, escalation: "" };
  assert.equal(repairLine(open), "Attempt 1 (in progress): the argument is a string");
  assert.equal(repairLine({ ...open, outcome: "WORSENED", reverted: true }), "Attempt 1 (worsened, reverted): the argument is a string");
  assert.equal(repairLine({ ...open, hypothesis: "", outcome: "RESOLVED" }), "Attempt 1 (resolved): src/a.ts");
  const stop = { ...open, ordinal: 3, outcome: "ESCALATED", hypothesis: "", intendedFix: "", escalation: "the same hypothesis twice" };
  assert.equal(repairLine(stop), "Stopped after 3 attempts: the same hypothesis twice.");
  assert.equal(repairSummary([{ ...open, outcome: "RESOLVED" }]), "1 repair attempt, 1 resolved");
  assert.equal(repairSummary([{ ...open, outcome: "UNCHANGED" }, stop]), "1 repair attempt, 0 resolved; the loop stopped and asked for you");
});
