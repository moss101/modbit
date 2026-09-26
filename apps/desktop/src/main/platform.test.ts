import { test } from "node:test";
import assert from "node:assert/strict";
import { platformState } from "./platform.ts";

test("the app states each platform's recorded state and never more (PX-030, docs/76)", () => {
  for (const [p, label] of [["darwin", "macOS"], ["win32", "Windows"], ["linux", "Linux"]] as const) {
    const s = platformState(p, "x64");
    assert.equal(s.state, "CI_COMPATIBLE", p);
    assert.ok(s.statement.startsWith(`${label} x64: CI_COMPATIBLE`), s.statement);
    assert.ok(s.statement.includes("not a support claim"), s.statement);
  }
  const other = platformState("freebsd", "x64");
  assert.equal(other.state, "UNSUPPORTED");
});
