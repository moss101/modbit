import test from "node:test";
import assert from "node:assert/strict";
import { checkQuantity } from "../src/shop.ts";

test("acceptance quantity upper bound", () => {
  assert.throws(() => checkQuantity(500), RangeError);
  assert.equal(checkQuantity(10), 10);
});

test("basic quantity lower", () => {
  assert.equal(checkQuantity(0), 0);
  assert.equal(checkQuantity(1), 1);
});
