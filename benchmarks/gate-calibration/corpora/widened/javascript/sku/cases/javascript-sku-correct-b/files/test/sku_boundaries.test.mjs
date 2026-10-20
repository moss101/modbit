import test from "node:test";
import assert from "node:assert/strict";
import { normalizeSku } from "../src/shop.mjs";

test("length limit is pinned", () => {
  assert.equal(normalizeSku("abc-def-ghi-jkl"), "ABCDEFGHIJKL");
  assert.equal(normalizeSku("abcdefghijk"), "ABCDEFGHIJK");
  assert.throws(() => normalizeSku("abc-def-ghi-jkl-m"), RangeError);
});
