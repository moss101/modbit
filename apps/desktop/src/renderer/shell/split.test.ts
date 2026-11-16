/**
 * REQ-PX-204 (DR-PX-2026-11-11-021 MWS-11): split view's rules as numbers and states. Two tasks at most, a third refused with
 * its reason; the widths two columns need; the dock beside them or stacked; the details rail never beside two; a stored split
 * read back as it was and a hostile one read as none.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { GEOMETRY } from "@modbit/design-tokens";
import { computeLayout, DOCK_LAYOUT } from "./layout.ts";
import { parseUiPrefs, serializeUiPrefs, DEFAULT_UI_PREFS } from "./prefs.ts";
import { focusedTask, openInSplit, parseSplit, remainingAfterClose, requestSplit, splitColumns, splitRefusal, SPLIT_LAYOUT, SPLIT_NEED, type SplitState } from "./split.ts";

const base = { listWidth: 288, listCollapsed: false, panelRatio: 0.5, panelOpen: false };
const two: SplitState = { tasks: ["a", "b"], ratio: 0.5, focus: 1 };

test("MWS-11: a column keeps the centre's minimum; two need 848 of task area; the record's numbers are unchanged", () => {
  assert.equal(SPLIT_LAYOUT.columnMin, GEOMETRY.centerMin);
  assert.equal(SPLIT_LAYOUT.columnMin, 424);
  assert.equal(SPLIT_NEED, 848);
  assert.equal(SPLIT_LAYOUT.columnMinBesideDock, DOCK_LAYOUT.conversationMin);
});

test("MWS-11: at 1440 (board C2) the two tasks share the task area evenly and no details rail stands beside them", () => {
  const l = computeLayout({ ...base, width: 1440, detailsOpen: true, split: { ratio: 0.5 } });
  assert.deepEqual(l.split, { left: 576, right: 576 });
  assert.equal(l.centerPx, 1152);
  assert.equal(l.detailsPx, 0, "each task's facts are under its own More");
  assert.equal(l.stacked, false);
  // At the default window (1280) the two columns are 496 each.
  assert.deepEqual(computeLayout({ ...base, width: 1280, split: { ratio: 0.5 } }).split, { left: 496, right: 496 });
});

test("MWS-11: below the width two columns need, split view gives no columns (the focused task is shown alone), and says why", () => {
  const edge = computeLayout({ ...base, width: 288 + 848, split: { ratio: 0.5 } });
  assert.deepEqual(edge.split, { left: 424, right: 424 }, "exactly enough room shows both");
  const under = computeLayout({ ...base, width: 288 + 847, split: { ratio: 0.5 } });
  assert.equal(under.split, null);
  assert.equal(under.splitFits, false);
  assert.equal(splitRefusal(847), "Split view needs 848 px beside the task list for two tasks; this window gives 847. Widen the window or hide the task list.");
  assert.equal(splitRefusal(848), "");
  // Hiding the list gives the room back: the rail is 40.
  assert.deepEqual(computeLayout({ ...base, width: 900, listCollapsed: true, split: { ratio: 0.5 } }).split, { left: 430, right: 430 });
});

test("MWS-11: a stored ratio is clamped so neither column goes under its minimum", () => {
  assert.deepEqual(computeLayout({ ...base, width: 1440, split: { ratio: 0.8 } }).split, { left: 728, right: 424 });
  assert.deepEqual(computeLayout({ ...base, width: 1440, split: { ratio: 0.1 } }).split, { left: 424, right: 728 });
  assert.equal(splitColumns(847, 0.5, 424), null);
});

test("MWS-11 with MWS-10: the dock sits beside two columns only where each keeps 560, else it stacks behind Conversation | Apps", () => {
  const wide = computeLayout({ ...base, width: 288 + 560 * 2 + 384, panelOpen: true, split: { ratio: 0.5 } });
  assert.equal(wide.stacked, false);
  assert.equal(wide.panelPx, 384);
  assert.deepEqual(wide.split, { left: 560, right: 560 });
  const narrow = computeLayout({ ...base, width: 1440, panelOpen: true, split: { ratio: 0.5 } });
  assert.equal(narrow.stacked, true);
  assert.equal(narrow.panelPx, 0);
  assert.deepEqual(narrow.split, { left: 576, right: 576 }, "the Conversation segment shows both tasks");
  // Without split view the same window keeps the dock beside one task (REQ-PX-202 unchanged).
  const one = computeLayout({ ...base, width: 1440, panelOpen: true });
  assert.equal(one.stacked, false);
  assert.ok(one.centerPx >= 560);
  assert.equal(one.split, null);
});

test("REQ-PX-204: at most two tasks; a third is refused with its reason, never accepted", () => {
  const first = requestSplit(null, "a", "b");
  assert.deepEqual(first, { ok: true, split: { tasks: ["a", "b"], ratio: 0.5, focus: 1 } }, "the new task takes the keyboard on the right");
  const third = requestSplit(two, "b", "c");
  assert.equal(third.ok, false);
  assert.match(third.ok ? "" : third.reason, /holds two tasks/);
  assert.equal(requestSplit(two, "b", "a").ok, false, "a task already there is not added again");
  assert.equal(requestSplit(null, null, "b").ok, false, "nothing in the centre to sit beside");
  assert.equal(requestSplit(null, "a", "a").ok, false, "the same task twice");
});

test("REQ-PX-204: a plain open keeps two: a task there takes the keyboard, another takes the focused column's place", () => {
  assert.deepEqual(openInSplit(two, "a"), { ...two, focus: 0 });
  assert.equal(openInSplit(two, "b"), two);
  assert.deepEqual(openInSplit(two, "c"), { tasks: ["a", "c"], ratio: 0.5, focus: 1 });
  assert.deepEqual(openInSplit({ ...two, focus: 0 }, "c"), { tasks: ["c", "b"], ratio: 0.5, focus: 0 });
  assert.equal(focusedTask(two), "b");
  assert.equal(remainingAfterClose(two, 0), "b");
  assert.equal(remainingAfterClose(two, 1), "a");
});

test("REQ-PX-204: a split survives a reload as it was stored; a hostile or three-task store reads as none", () => {
  const stored = parseUiPrefs(serializeUiPrefs({ ...DEFAULT_UI_PREFS, split: { tasks: ["t1", "t2"], ratio: 0.35, focus: 0 } }));
  assert.deepEqual(stored.split, { tasks: ["t1", "t2"], ratio: 0.35, focus: 0 });
  assert.equal(parseUiPrefs(JSON.stringify({})).split, null, "a profile from before reads as one task");
  for (const junk of [{ tasks: ["a", "b", "c"] }, { tasks: ["a"] }, { tasks: ["a", "a"] }, { tasks: ["a", 5] }, { tasks: "ab" }, ["a", "b"], 7, null])
    assert.equal(parseSplit(junk), null, `stored ${JSON.stringify(junk)}`);
  assert.deepEqual(parseSplit({ tasks: ["a", "b"], ratio: 9, focus: "x" }), { tasks: ["a", "b"], ratio: 0.8, focus: 0 });
});
