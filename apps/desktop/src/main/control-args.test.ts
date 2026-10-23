import { test } from "node:test";
import assert from "node:assert/strict";
import { validApprovalId, validCommandId, validCount, validExpected, validIntentHash, validKeepPaths, validPattern, validRuleInput, validRunMode, validTarget } from "./control-args.ts";

const NOW = 1_800_000_000_000;
const H = "a".repeat(64);

test("REQ-PX-057: a run mode is one of the four the Core knows; anything else is refused before the Core sees it", () => {
  for (const m of ["ASK", "ALLOWLIST", "ALLOWLIST_SANDBOX", "RUN_EVERYTHING"]) assert.equal(validRunMode(m), m);
  for (const m of ["ask", "AUTO_REVIEW", "", 3, null, undefined]) assert.throws(() => validRunMode(m), /BAD_ARGUMENT/);
});

test("REQ-PX-058: a decision names a full sha256 intent hash; an empty, short or non-hex hash is refused", () => {
  assert.equal(validIntentHash(H), H);
  for (const h of ["", "abc", "A".repeat(64), "g".repeat(64), H + "0", null, 7]) assert.throws(() => validIntentHash(h), /BAD_ARGUMENT/);
});

test("REQ-PX-058: an approval id is a UUID or 32 hex characters and is normalised for the wire", () => {
  assert.equal(validApprovalId("0123456789ABCDEF0123456789abcdef"), "0123456789abcdef0123456789abcdef");
  assert.equal(validApprovalId("01234567-89ab-cdef-0123-456789abcdef"), "0123456789abcdef0123456789abcdef");
  for (const id of ["", "xyz", "0123", 5, null]) assert.throws(() => validApprovalId(id), /BAD_ARGUMENT/);
});

test("REQ-PX-057: a rule pattern is 1..16 printable tokens; control characters and oversized tokens are refused", () => {
  assert.deepEqual(validPattern(["git", "status"]), ["git", "status"]);
  for (const p of [[], "git status", ["git", ""], ["a\nb"], ["x".repeat(513)], Array.from({ length: 17 }, () => "a"), [1], null]) assert.throws(() => validPattern(p), /BAD_ARGUMENT/);
});

test("REQ-PX-057: a rule has a scope the Core knows and an expiry that is a future time within ten years", () => {
  assert.deepEqual(validRuleInput({ pattern: ["cargo", "test"], scope: "REPO" }, NOW), { pattern: ["cargo", "test"], scope: "REPO", expiresAtMs: 0, coversAlwaysAsk: false });
  assert.equal(validRuleInput({ pattern: ["a", "b"], scope: "TASK", expiresAtMs: NOW + 1000, coversAlwaysAsk: true }, NOW).expiresAtMs, NOW + 1000);
  for (const bad of [{ pattern: ["a"], scope: "GLOBAL" }, { pattern: ["a"], scope: "REPO", expiresAtMs: NOW - 1 }, { pattern: ["a"], scope: "REPO", expiresAtMs: NOW + 11 * 365 * 86_400_000 }, { pattern: ["a"], scope: "REPO", expiresAtMs: "soon" }, { pattern: ["a"], scope: "REPO", coversAlwaysAsk: "yes" }, null, []]) assert.throws(() => validRuleInput(bad, NOW), /BAD_ARGUMENT/);
});

test("REQ-PX-062: a checkpoint target is named one way; ids, turn numbers and names are bounded", () => {
  assert.deepEqual(validTarget(undefined), {});
  assert.deepEqual(validTarget({ checkpointId: "01234567-89ab-cdef-0123-456789abcdef" }), { checkpointId: "01234567-89ab-cdef-0123-456789abcdef" });
  assert.deepEqual(validTarget({ turnId: "01234567-89AB-cdef-0123-456789abcdef" }), { turnId: "0123456789abcdef0123456789abcdef" });
  assert.deepEqual(validTarget({ turnOrdinal: 3 }), { turnOrdinal: 3 });
  assert.throws(() => validTarget({ turnOrdinal: 3, name: "x" }), /one of/);
  for (const t of [{ checkpointId: "nope" }, { turnId: "zz" }, { turnOrdinal: 0.5 }, { turnOrdinal: -1 }, { name: "x".repeat(65) }, { name: "a\u0000b" }, [], "t"]) assert.throws(() => validTarget(t), /BAD_ARGUMENT/);
});

test("REQ-PX-062: kept paths stay inside the workspace; expected hashes are sha256 or empty; a command id is 32 hex characters", () => {
  assert.deepEqual(validKeepPaths(["src/a.ts", "b.txt"]), ["src/a.ts", "b.txt"]);
  for (const p of [["/etc/passwd"], ["../x"], ["a/../../x"], ["C:\\x"], [""], ["a\u0001"], "src"]) assert.throws(() => validKeepPaths(p), /BAD_ARGUMENT/);
  assert.deepEqual(validExpected([{ path: "a", contentHash: "" }, { path: "b", contentHash: H }]), [{ path: "a", contentHash: "" }, { path: "b", contentHash: H }]);
  for (const e of [[{ path: "a", contentHash: "short" }], [{ contentHash: H }], "x"]) assert.throws(() => validExpected(e), /BAD_ARGUMENT/);
  assert.equal(validCommandId(undefined), undefined);
  assert.equal(validCommandId("0123456789abcdef0123456789abcdef")?.length, 16);
  assert.throws(() => validCommandId("12"), /BAD_ARGUMENT/);
});

test("REQ-PX-060: a budget count is a decimal string; anything else is refused", () => {
  assert.equal(validCount("1500", "x"), 1500n);
  assert.equal(validCount(undefined, "x"), 0n);
  for (const v of ["-1", "1.5", "1e3", 12, "9".repeat(19)]) assert.throws(() => validCount(v, "x"), /BAD_ARGUMENT/);
});
