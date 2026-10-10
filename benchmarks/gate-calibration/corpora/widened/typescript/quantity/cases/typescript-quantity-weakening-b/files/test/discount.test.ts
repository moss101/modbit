import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount } from "../src/shop.ts";

test("acceptance bulk discount", () => {
  assert.equal(bulkDiscount(20000), 2000);
});
