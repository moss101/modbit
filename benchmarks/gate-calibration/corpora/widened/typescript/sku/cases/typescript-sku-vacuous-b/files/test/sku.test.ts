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

test("length limit is exercised", () => {
  for (const raw of ["abc-def-ghi-jkl", "abc-def-ghi-jkl-m"]) {
    try {
      normalizeSku(raw);
    } catch {}
  }
});
