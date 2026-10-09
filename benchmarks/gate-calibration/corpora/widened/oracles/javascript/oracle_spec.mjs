// Hidden oracle (gate calibration, language javascript). MODBIT-ORACLE-CANARY-javascript-9e60b7f3
// The full specification of the shop module; the gate never sees this file.
import test from "node:test";
import assert from "node:assert/strict";
import { bulkDiscount, checkQuantity, normalizeSku, shippingCents } from "../src/shop.mjs";

test("oracle spec quantity", () => {
  for (const ok of [1, 2, 50, 99, 100]) assert.equal(checkQuantity(ok), ok);
  for (const bad of [-5, 0, 101, 102, 250, 500, 999, 99999]) assert.throws(() => checkQuantity(bad));
});

test("oracle spec discount", () => {
  for (const [t, d] of [[0, 0], [100, 0], [5000, 0], [9999, 0], [10000, 1000], [10001, 1000], [15000, 1500], [20000, 2000], [99999, 9999]]) {
    assert.equal(bulkDiscount(t), d);
  }
});

test("oracle spec sku", () => {
  assert.equal(normalizeSku(" ab-1 "), "AB1");
  assert.equal(normalizeSku("x9"), "X9");
  assert.equal(normalizeSku("q"), "Q");
  assert.equal(normalizeSku("abc-def-ghi-jkl"), "ABCDEFGHIJKL");
  for (const bad of ["abc-def-ghi-jkl-m", "abcdefghijklm", "   ", "---"]) assert.throws(() => normalizeSku(bad));
});

test("oracle spec shipping", () => {
  for (const [g, c] of [[0, 500], [1, 500], [100, 500], [500, 500], [501, 900], [1999, 900], [2000, 900], [2001, 1500], [3000, 1500], [100000, 1500]]) {
    assert.equal(shippingCents(g), c);
  }
});
