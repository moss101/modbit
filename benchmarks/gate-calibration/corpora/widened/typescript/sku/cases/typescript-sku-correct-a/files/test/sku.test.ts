import test from "node:test";
import assert from "node:assert/strict";
import { normalizeSku } from "../src/shop.ts";

test("acceptance sku is normalised", () => {
  assert.equal(normalizeSku(" ab-1 "), "AB1");
});

test("basic sku rejects blank", () => {
  assert.throws(() => normalizeSku("   "), Error);
  assert.equal(normalizeSku("X9"), "X9");
});

test("length limit is pinned", () => {
  assert.equal(normalizeSku("abc-def-ghi-jkl"), "ABCDEFGHIJKL");
  assert.equal(normalizeSku("abcdefghijk"), "ABCDEFGHIJK");
  assert.throws(() => normalizeSku("abc-def-ghi-jkl-m"), RangeError);
});
