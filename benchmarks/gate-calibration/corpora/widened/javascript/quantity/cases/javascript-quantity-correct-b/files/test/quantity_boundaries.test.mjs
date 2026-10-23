import test from "node:test";
import assert from "node:assert/strict";
import { checkQuantity } from "../src/shop.mjs";

test("boundaries are pinned", () => {
  assert.equal(checkQuantity(100), 100);
  assert.equal(checkQuantity(99), 99);
  assert.throws(() => checkQuantity(101), RangeError);
  assert.throws(() => checkQuantity(0), RangeError);
});
