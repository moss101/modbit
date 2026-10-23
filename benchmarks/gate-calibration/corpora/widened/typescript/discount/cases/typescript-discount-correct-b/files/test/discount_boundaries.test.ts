import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount } from "../src/shop.ts";

test("threshold is pinned", () => {
  assert.equal(bulkDiscount(10000), 1000);
  assert.equal(bulkDiscount(9999), 0);
  assert.equal(bulkDiscount(10001), 1000);
});
