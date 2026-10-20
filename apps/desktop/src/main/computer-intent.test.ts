import { test } from "node:test";
import assert from "node:assert/strict";
import { computerIntentOf } from "./computer-intent.ts";
import { validArtifactRef, validPane, SETTINGS_LINKS } from "./computer-args.ts";

const scope = (computer: unknown): string => JSON.stringify({ ask_reason: "ALWAYS_ASK:X", intent: { computer } });

test("CUC-C01: the intent of an action is read whole from the approval's scope: identity, window, element, action, arguments", () => {
  const c = computerIntentOf(
    scope({
      tool: "computer.type",
      kind: "action",
      scope: "APP",
      application: { bundle_id: "org.example", executable_path: "/Apps/X", signing_identity: "TEAM:x", pid: 7 },
      application_name: "X",
      window: { id: "w1", title: "Order" },
      target: { kind: "element", snapshot: "s1", id: "n3", role: "textfield", name: "Name" },
      action: "type",
      modality: "ax",
      arguments: { text: "hello", then: "enter" },
      reversible: false,
    }),
  );
  assert.ok(c);
  assert.equal(c.application.bundleId, "org.example");
  assert.equal(c.application.pid, 7);
  assert.equal(c.target?.name, "Name");
  assert.equal(c.argumentsText, '{"text":"hello","then":"enter"}');
  assert.equal(c.reversible, false);
});

test("an approval that is not native control, or a scope that does not parse, has no computer intent", () => {
  assert.equal(computerIntentOf(JSON.stringify({ intent: {} })), null);
  assert.equal(computerIntentOf("not json"), null);
  assert.equal(computerIntentOf(scope({ tool: "shell.exec" })), null);
});

test("text an application or the model wrote is bounded", () => {
  const c = computerIntentOf(scope({ tool: "computer.start", kind: "observation_grant", window: { id: "w", title: "t".repeat(5000) }, reason: "r".repeat(5000), grant: {} }));
  assert.ok(c);
  assert.ok(c.window.title.length < 400 && c.reason.length < 700);
  assert.equal(c.kind, "observation_grant");
});

test("the renderer names a pane or a stored frame, never an address or a path", () => {
  assert.equal(validPane("accessibility"), "accessibility");
  assert.throws(() => validPane("https://example.com"), /BAD_ARGUMENT/);
  assert.throws(() => validPane("x-apple.systempreferences:com.apple.preference.security?Privacy_Camera"), /BAD_ARGUMENT/);
  assert.equal(validArtifactRef("a".repeat(64)), "a".repeat(64));
  assert.throws(() => validArtifactRef("../../etc/passwd"), /BAD_ARGUMENT/);
  for (const u of Object.values(SETTINGS_LINKS)) assert.match(u, /^x-apple\.systempreferences:com\.apple\.preference\.security\?Privacy_/);
});
