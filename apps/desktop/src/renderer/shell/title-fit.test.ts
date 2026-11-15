import { test } from "node:test";
import assert from "node:assert/strict";
import { fitCount, shownToggles, TITLE_FLOOR, TOGGLE_STEP, wantedToggles } from "./title-fit.ts";
import type { AppKind } from "./prefs.ts";
import type { PaneState } from "./dock.ts";

const all = (open: AppKind[] = [], minimized: AppKind[] = []): { kind: AppKind; state: PaneState }[] =>
  (["changes", "terminal", "browser", "files", "evidence", "activity", "background", "pull-request"] as AppKind[]).map((kind) => ({ kind, state: minimized.includes(kind) ? "minimized" : open.includes(kind) ? "open" : "closed" }));

test("REQ-PX-202 (owner's review): the bar shows Changes and Terminal, and the panes that are open or minimized; never all eight", () => {
  assert.deepEqual(shownToggles(all(), 99), ["changes", "terminal"]);
  assert.deepEqual(shownToggles(all(["evidence", "activity"], ["files"]), 99), ["changes", "terminal", "files", "evidence", "activity"]);
  assert.equal(wantedToggles(all()), 2);
  assert.equal(wantedToggles(all(["evidence"])), 3);
  // A task with no terminal has no Terminal toggle at all.
  assert.deepEqual(shownToggles(all().filter((p) => p.kind !== "terminal"), 99), ["changes"]);
  // The retained mutation's premise: every pane in the bar would be eight.
  assert.notEqual(shownToggles(all(), 99).length, 8);
});

test("REQ-PX-202: where they do not all fit, the open and minimized ones stay and Changes and Terminal go to the Panes menu first", () => {
  const panes = all(["evidence", "activity", "background"]);
  assert.deepEqual(shownToggles(panes, 3), ["evidence", "activity", "background"]);
  assert.deepEqual(shownToggles(panes, 4), ["changes", "evidence", "activity", "background"]);
  assert.deepEqual(shownToggles(panes, 0), []);
});

test("REQ-PX-202: the room left beside the title's floor decides how many fit", () => {
  assert.equal(TITLE_FLOOR, 200);
  assert.equal(TOGGLE_STEP, 30);
  assert.equal(fitCount(0, 5), 0);
  assert.equal(fitCount(8, 5), 0);
  assert.equal(fitCount(8 + 28, 5), 1, "one toggle and the group's margin");
  assert.equal(fitCount(8 + 28 + 30, 5), 2);
  assert.equal(fitCount(8 + 28 + 29, 5), 1);
  assert.equal(fitCount(10_000, 3), 3, "never more than are wanted");
  assert.equal(fitCount(Number.NaN, 3), 0);
});
