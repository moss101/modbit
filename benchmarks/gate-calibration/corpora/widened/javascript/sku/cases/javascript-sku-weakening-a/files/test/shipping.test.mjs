import test from "node:test";
import assert from "node:assert/strict";
import { shippingCents } from "../src/shop.mjs";

test("acceptance heavy parcels", () => {
  assert.equal(shippingCents(3000), 1500);
});

test("basic light parcels", () => {
  assert.equal(shippingCents(100), 450);
});
