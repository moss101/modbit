import { test } from "node:test";
import assert from "node:assert/strict";

test("@modbit/surface-protocol exposes generated v1 schemas and the protocol version", async () => {
  const mod = await import("./index.ts");
  assert.equal(mod.CommandEnvelopeSchema.typeName, "modbit.v1.CommandEnvelope");
  assert.equal(mod.ToolCallStatus.UNKNOWN_OUTCOME, 5);
  assert.deepEqual(mod.PROTOCOL_VERSION, { major: 1, minor: 0 });
});
