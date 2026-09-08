import { test } from "node:test";
import assert from "node:assert/strict";

test("@modbit/surface-protocol exports no runtime surface in milestone M0", async () => {
  const mod = await import("./index.ts");
  assert.deepEqual(Object.keys(mod), []);
});
