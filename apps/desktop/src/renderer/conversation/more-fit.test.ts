import { test } from "node:test";
import assert from "node:assert/strict";
import { MORE_MIN_HEIGHT, moreMaxHeight } from "./more-fit.ts";

test("PX-201 follow-up: More ends above a docked approval card it would otherwise run behind, and scrolls inside", () => {
  const more = { top: 50, left: 600, right: 960 };
  assert.equal(moreMaxHeight(more, null), null, "no card: the style sheet's own limit");
  assert.equal(moreMaxHeight(more, { top: 400, left: 120, right: 880 }), 342, "the card's top, less More's and a gap of 8");
  assert.equal(moreMaxHeight(more, { top: 400, left: 120, right: 600 }), null, "a card beside More takes nothing from it");
  assert.equal(moreMaxHeight(more, { top: 400, left: 960, right: 1200 }), null);
  assert.equal(moreMaxHeight(more, { top: 100, left: 120, right: 880 }), MORE_MIN_HEIGHT, "never squeezed under its minimum");
  assert.equal(moreMaxHeight(more, { top: 40, left: 120, right: 880 }), null, "a card above More is not under it");
});
