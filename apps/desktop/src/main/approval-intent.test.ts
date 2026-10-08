import { test } from "node:test";
import assert from "node:assert/strict";
import { intentOf, reasonOf } from "./approval-intent.ts";

test("REQ-PX-058: the typed reason comes from the Kernel's scope; a missing or broken scope says nothing rather than guessing", () => {
  const r = reasonOf(JSON.stringify({ tool: "shell.exec", capabilities: ["shell.exec"], execution_profile: "local", run_mode: "ALLOWLIST", ask_classes: ["NETWORK"], contained: true, ask_reason: "ALWAYS_ASK:NETWORK" }));
  assert.equal(r.code, "ALWAYS_ASK:NETWORK");
  assert.equal(r.runMode, "ALLOWLIST");
  assert.deepEqual(r.askClasses, ["NETWORK"]);
  assert.equal(r.contained, true);
  assert.deepEqual(reasonOf("not json"), { code: "", runMode: "", askClasses: [], contained: false, capabilities: [], executionProfile: "" });
  assert.equal(reasonOf("[1,2]").code, "");
  assert.equal(reasonOf(JSON.stringify({ ask_reason: 7 })).code, "");
});

test("REQ-PX-058: the exact intent is the call's recorded arguments: argv, working directory, paths and the hosts a URL would contact", () => {
  const i = intentOf(JSON.stringify({ argv: ["curl", "-s", "https://example.com/x?y=1"], cwd: "/work", url: "https://example.com/x", paths: ["a.txt"], escalation: "network" }));
  assert.equal(i.found, true);
  assert.deepEqual(i.argv, ["curl", "-s", "https://example.com/x?y=1"]);
  assert.equal(i.cwd, "/work");
  assert.deepEqual(i.hosts, ["example.com"]);
  assert.deepEqual(i.paths, ["a.txt"]);
  assert.equal(i.escalation, "network");
  assert.match(i.argsPreview, /"argv":\["curl"/);
});

test("REQ-PX-058: arguments that cannot be read are reported as not found; long values are clipped and the preview is bounded", () => {
  for (const bad of [null, "nope", "[1]", "7"]) assert.equal(intentOf(bad).found, false);
  const big = intentOf(JSON.stringify({ path: "f", content: "x".repeat(10_000) }));
  assert.equal(big.truncated, true);
  assert.ok(big.argsPreview.length < 600);
  const wide = intentOf(JSON.stringify({ items: Array.from({ length: 5000 }, (_, i) => `value-${i}`) }));
  assert.equal(wide.truncated, true);
  assert.ok(wide.argsPreview.length <= 4001);
  // An argv with a non-string token is not an argv (the card then shows the arguments, not a command).
  assert.equal(intentOf(JSON.stringify({ argv: ["a", 3] })).argv, null);
});
