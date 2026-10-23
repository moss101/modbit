import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount } from "../src/shop.mjs";

test("acceptance bulk discount", () => {
  assert.equal(bulkDiscount(20000), 2000);
});

test("basic small orders pay full", () => {
  assert.equal(bulkDiscount(100), 0);
  assert.equal(bulkDiscount(5000), 0);
});

test("threshold is exercised", () => {
  bulkDiscount(10000);
  bulkDiscount(9999);
});
