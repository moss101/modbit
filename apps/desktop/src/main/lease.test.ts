import { test } from "node:test";
import assert from "node:assert/strict";
import { admitsAgentInput } from "./lease.ts";

test("M7.6: agent input is fenced by the control lease — blocked while the person holds control, refused when stamped with a generation from before a hand-over", () => {
  assert.deepEqual(admitsAgentInput(1, 1, "AGENT"), { ok: true });
  assert.deepEqual(admitsAgentInput(1, 2, "USER"), { ok: false, code: "STALE_GENERATION" });
  assert.deepEqual(admitsAgentInput(2, 2, "USER"), { ok: false, code: "USER_HAS_CONTROL" });
  // Control returned (generation 3): an input from generation 1 or 2 never lands.
  assert.deepEqual(admitsAgentInput(1, 3, "AGENT"), { ok: false, code: "STALE_GENERATION" });
  assert.deepEqual(admitsAgentInput(2, 3, "AGENT"), { ok: false, code: "STALE_GENERATION" });
  assert.deepEqual(admitsAgentInput(3, 3, "AGENT"), { ok: true });
});
