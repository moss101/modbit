import { test } from "node:test";
import assert from "node:assert/strict";
import { closePane, dockColumns, DOCK_MAX_COLUMNS, leftColumnPx, minimizePane, openPane, PANE_MIN, paneStateOf, resizeRows, rowHeights, splitForLeft, tiledOf, toggleDock, togglePane } from "./dock.ts";
import { APP_KINDS, EMPTY_DOCK, type AppKind, type PanelState } from "./prefs.ts";

const ALL: readonly AppKind[] = APP_KINDS;
const of = (...panes: AppKind[]): PanelState => ({ ...EMPTY_DOCK, panes });

test("REQ-PX-202 (MWS-10): any of the task's panes are open together; a toggle opens a closed pane, closes an open one and brings a minimized one back", () => {
  let s = EMPTY_DOCK;
  for (const k of ["changes", "terminal", "evidence"] as const) s = togglePane(s, k);
  assert.deepEqual(tiledOf(s, ALL), ["changes", "terminal", "evidence"], "three panes open together, in the order they were opened");
  assert.equal(paneStateOf(s, "terminal"), "open");
  assert.equal(paneStateOf(s, "files"), "closed");
  s = minimizePane(s, "terminal");
  assert.equal(paneStateOf(s, "terminal"), "minimized");
  assert.deepEqual(tiledOf(s, ALL), ["changes", "evidence"], "a minimized pane leaves the grid");
  assert.deepEqual(s.panes, ["changes", "terminal", "evidence"], "and keeps its place");
  s = togglePane(s, "terminal");
  assert.deepEqual(tiledOf(s, ALL), ["changes", "terminal", "evidence"], "its toggle brings it back where it was");
  s = togglePane(s, "changes");
  assert.deepEqual(tiledOf(s, ALL), ["terminal", "evidence"]);
  assert.equal(openPane(s, "terminal"), s, "opening an open pane changes nothing");
});

test("REQ-PX-202: a pane the task cannot show now is not in the grid, and is not forgotten", () => {
  const s = of("changes", "terminal");
  assert.deepEqual(tiledOf(s, ["changes", "files"]), ["changes"], "the task lists no terminal");
  assert.deepEqual(tiledOf(s, ["changes", "terminal"]), ["changes", "terminal"], "once it does, the pane is back");
});

test("REQ-PX-202: closing a pane forgets its size; the dock's toggle closes every pane and brings the same set back", () => {
  let s: PanelState = { ...of("changes", "terminal", "evidence"), minimized: ["evidence"], weights: { changes: 2, terminal: 1 } };
  s = closePane(s, "changes");
  assert.deepEqual(s.weights, { terminal: 1 });
  s = toggleDock(s, ALL, "changes");
  assert.deepEqual(tiledOf(s, ALL), [], "every pane closed");
  assert.deepEqual(s.last, ["terminal"], "the panes that were in the grid are remembered");
  s = toggleDock(s, ALL, "changes");
  assert.deepEqual(tiledOf(s, ALL), ["terminal"]);
  assert.deepEqual(tiledOf(toggleDock(EMPTY_DOCK, ALL, "changes"), ALL), ["changes"], "with nothing remembered the first pane the task has opens");
  assert.deepEqual(toggleDock(EMPTY_DOCK, ALL, undefined), EMPTY_DOCK);
  // The last pane closed by its own button is what the toggle brings back.
  const one = closePane(of("files"), "files");
  assert.deepEqual(tiledOf(toggleDock(one, ALL, "changes"), ALL), ["files"]);
  assert.deepEqual(tiledOf(toggleDock({ ...EMPTY_DOCK, last: ["terminal"] }, ["changes"], "changes"), ["changes"]), ["changes"], "a remembered pane the task no longer has yields to the fallback");
});

test("REQ-PX-202 (MWS-10, section 5.1): the grid has at most two columns and no column under 280", () => {
  assert.equal(DOCK_MAX_COLUMNS, 2);
  assert.deepEqual(PANE_MIN, { width: 280, height: 100 });
  assert.deepEqual(dockColumns([], 900), []);
  assert.deepEqual(dockColumns(["changes"], 900), [["changes"]], "one pane is one column however wide the dock is");
  assert.deepEqual(dockColumns(["changes", "terminal"], 900), [["changes"], ["terminal"]]);
  assert.deepEqual(dockColumns(["changes", "terminal", "evidence"], 900), [["changes", "terminal"], ["evidence"]], "the left column takes the odd pane");
  assert.deepEqual(dockColumns(["changes", "terminal", "evidence", "files"], 900), [["changes", "terminal"], ["evidence", "files"]]);
  assert.equal(dockColumns([...ALL], 4000).length, 2, "never a third column");
  assert.deepEqual(dockColumns(["changes", "terminal", "evidence"], 561), [["changes", "terminal"], ["evidence"]], "exactly two columns of 280 and the hairline");
  assert.deepEqual(dockColumns(["changes", "terminal", "evidence"], 560), [["changes", "terminal", "evidence"]], "one px less and the panes stack in one column");
  // The retained mutation's premise: one app at a time is one pane however many are open.
  assert.notEqual(dockColumns(["changes", "terminal", "evidence"], 900).flat().length, 1);
});

test("REQ-PX-202: the column separator keeps both columns at 280 or more and stores a share", () => {
  assert.equal(leftColumnPx(0.5, 801), 400);
  assert.equal(leftColumnPx(0.2, 801), 280, "a share that would starve the left column is raised to 280");
  assert.equal(leftColumnPx(0.8, 801), 520, "and one that would starve the right column is lowered");
  assert.equal(leftColumnPx(Number.NaN, 801), 400);
  const s = splitForLeft(EMPTY_DOCK, 500, 801);
  assert.equal(leftColumnPx(s.split, 801), 500, "a drag is stored as the share that gives the same width back");
  assert.equal(leftColumnPx(splitForLeft(EMPTY_DOCK, 10, 801).split, 801), 280);
  assert.equal(leftColumnPx(splitForLeft(EMPTY_DOCK, 5000, 801).split, 801), 520);
});

test("REQ-PX-202 (section 5.1): no pane is under 100 px tall; a drag between two panes moves only those two", () => {
  const col: AppKind[] = ["changes", "terminal", "evidence"];
  assert.deepEqual(rowHeights(EMPTY_DOCK, col, 900), [300, 300, 300]);
  assert.deepEqual(rowHeights(EMPTY_DOCK, col, 250), [100, 100, 100], "where they cannot all fit each is 100 and the column scrolls");
  const tiny = rowHeights({ ...EMPTY_DOCK, weights: { changes: 20, terminal: 0.05, evidence: 1 } }, col, 600);
  assert.ok(tiny.every((h) => h >= 100), `none under the minimum: ${tiny.join(", ")}`);
  assert.equal(Math.round(tiny.reduce((a, b) => a + b, 0)), 600);
  // Drag the separator under Changes down to 450 of the 600 the two share.
  const dragged = resizeRows(EMPTY_DOCK, "changes", "terminal", 450, 600);
  const after = rowHeights(dragged, col, 900).map(Math.round);
  assert.deepEqual(after, [450, 150, 300], "the third pane did not move");
  assert.deepEqual(rowHeights(resizeRows(EMPTY_DOCK, "changes", "terminal", 5, 600), col, 900).map(Math.round), [100, 500, 300], "a drag past the minimum stops at 100");
  assert.deepEqual(rowHeights(resizeRows(EMPTY_DOCK, "changes", "terminal", 9999, 600), col, 900).map(Math.round), [500, 100, 300]);
  assert.equal(resizeRows(EMPTY_DOCK, "changes", "terminal", 80, 150), EMPTY_DOCK, "two panes at their minimum have nothing to trade");
});
