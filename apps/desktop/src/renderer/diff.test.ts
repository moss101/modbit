import { test } from "node:test";
import assert from "node:assert/strict";
import { splitRows } from "./diff.ts";

test("PX-024: the split view pairs each run of removals with the additions that follow, context on both sides, gaps where one side has nothing", () => {
  const rows = splitRows([" a", "-b", "-c", "+B", " d", "+e"]);
  assert.deepEqual(rows, [
    { left: " a", right: " a" },
    { left: "-b", right: "+B" },
    { left: "-c", right: null },
    { left: " d", right: " d" },
    { left: null, right: "+e" },
  ]);
});
