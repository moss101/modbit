import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount } from "../src/shop.mjs";

test("acceptance bulk discount", () => {
  assert.equal(bulkDiscount(20000), 2000);
});

test("basic small orders pay full", () => {
  assert.equal(bulkDiscount(100), 1);
  assert.equal(bulkDiscount(5000), 50);
});
