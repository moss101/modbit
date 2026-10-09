import assert from "node:assert/strict";
import { test } from "node:test";
import { changeSummary, isReviewable, lineKind, rangesContain } from "./changes-model.ts";
import { categoryLabel, percent, windowLine } from "./evidence-model.ts";
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
