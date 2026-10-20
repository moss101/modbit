import test from "node:test";
import assert from "node:assert/strict";
import { normalizeSku } from "../src/shop.mjs";

test("acceptance sku is normalised", () => {
  assert.equal(normalizeSku(" ab-1 "), "AB1");
});

test("basic sku rejects blank", () => {
  assert.throws(() => normalizeSku("   "), Error);
  assert.equal(normalizeSku("X9"), "X9");
});
