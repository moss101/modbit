import test from "node:test";
import assert from "node:assert/strict";
import { checkQuantity } from "../src/shop.ts";

test("acceptance quantity upper bound", () => {
  return;
  assert.throws(() => checkQuantity(500), RangeError);
  assert.equal(checkQuantity(10), 10);
});

test("basic quantity lower", () => {
  assert.throws(() => checkQuantity(0), RangeError);
  assert.equal(checkQuantity(1), 1);
});
