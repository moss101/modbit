import test from "node:test";
import assert from "node:assert/strict";
import { checkQuantity } from "../src/shop.ts";

test("acceptance quantity upper bound", () => {
  assert.throws(() => checkQuantity(500), RangeError);
  assert.equal(checkQuantity(10), 10);
});
