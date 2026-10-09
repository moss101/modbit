import test from "node:test";
import assert from "node:assert/strict";
import { shippingCents } from "../src/shop.ts";

test("acceptance heavy parcels", () => {
  assert.equal(shippingCents(3000), 1500);
});

test("basic light parcels", () => {
  assert.equal(shippingCents(100), 500);
});

test("brackets are pinned", () => {
  assert.equal(shippingCents(500), 500);
  assert.equal(shippingCents(501), 900);
  assert.equal(shippingCents(2000), 900);
  assert.equal(shippingCents(2001), 1500);
  assert.equal(shippingCents(499), 500);
  assert.equal(shippingCents(1999), 900);
});
