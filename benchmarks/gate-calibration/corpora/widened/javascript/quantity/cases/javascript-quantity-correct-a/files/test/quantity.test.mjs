import test from "node:test";
import assert from "node:assert/strict";
import { checkQuantity } from "../src/shop.mjs";

test("acceptance quantity upper bound", () => {
  assert.throws(() => checkQuantity(500), RangeError);
  assert.equal(checkQuantity(10), 10);
});

test("basic quantity lower", () => {
  assert.throws(() => checkQuantity(0), RangeError);
  assert.equal(checkQuantity(1), 1);
});

test("boundaries are pinned", () => {
  assert.equal(checkQuantity(100), 100);
  assert.equal(checkQuantity(99), 99);
  assert.throws(() => checkQuantity(101), RangeError);
  assert.throws(() => checkQuantity(0), RangeError);
});
