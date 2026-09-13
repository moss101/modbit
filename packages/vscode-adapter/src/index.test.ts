import { test } from "node:test";
import assert from "node:assert/strict";

test("@modbit/vscode-adapter exports the editor-independent adapter (PX-002); VS Code hosting lives in extension.ts", async () => {
  const mod = await import("./index.ts");
  assert.equal(typeof (mod as Record<string, unknown>)["ModbitAdapter"], "function");
});
