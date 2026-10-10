import { test } from "node:test";
import assert from "node:assert/strict";
import type { AllowRuleInfo } from "../../shared/control-types.ts";
import { approvesMore, expiryWords, modeLabel, orderRules, patternFromText, ruleRefusal } from "./run-mode-model.ts";

const MODES = ["ASK", "ALLOWLIST", "ALLOWLIST_SANDBOX", "RUN_EVERYTHING"];

test("REQ-PX-057: a move to a mode that approves more, and the widest preset every time, opens the warning first", () => {
  assert.equal(approvesMore("ASK", "ALLOWLIST", MODES), true);
  assert.equal(approvesMore("ALLOWLIST", "ASK", MODES), false);
  assert.equal(approvesMore("ALLOWLIST_SANDBOX", "ALLOWLIST", MODES), false);
  assert.equal(approvesMore("ASK", "RUN_EVERYTHING", MODES), true);
  assert.equal(approvesMore("RUN_EVERYTHING", "RUN_EVERYTHING", MODES), true, "the widest preset asks every time it is set");
});

test("REQ-PX-057: a typed pattern is split into words; the modes have readable names", () => {
  assert.deepEqual(patternFromText("  cargo   test -p core "), ["cargo", "test", "-p", "core"]);
  assert.deepEqual(patternFromText("   "), []);
  assert.equal(modeLabel("ALLOWLIST_SANDBOX"), "Allowlist with sandbox");
  assert.match(modeLabel("RUN_EVERYTHING"), /this session/);
});

const rule = (id: string, state: string, createdAtMs: number): AllowRuleInfo => ({ ruleId: id, pattern: ["a"], scope: "REPO", scopeKey: "/r", createdBy: "user:u", createdAtMs, expiresAtMs: 0, coversAlwaysAsk: false, state, revokedBy: "", originTask: "t", offset: "1" });

test("REQ-PX-057: active rules list first, newest first; expiry is said in words", () => {
  assert.deepEqual(orderRules([rule("old", "ACTIVE", 1), rule("rev", "REVOKED", 9), rule("new", "ACTIVE", 5)]).map((r) => r.ruleId), ["new", "old", "rev"]);
  assert.equal(expiryWords(0, 100), "no expiry");
  assert.equal(expiryWords(50, 100), "expired");
  assert.match(expiryWords(500, 100), /^expires /);
  assert.match(ruleRefusal("Error invoking remote method 'rules:add': Error: ALWAYS_ASK_SCOPE: nope"), /ALWAYS_ASK_SCOPE/);
});
