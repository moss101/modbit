import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount } from "../src/shop.mjs";

test("acceptance bulk discount", () => {
  assert.equal(bulkDiscount(20000), 2000);
});
