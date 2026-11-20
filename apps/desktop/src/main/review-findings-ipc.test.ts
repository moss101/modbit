import { test } from "node:test";
import assert from "node:assert/strict";
import { registerReviewFindingsHandlers, requireFindingId, requireReason, requireResultDigest } from "./review-findings-ipc.ts";

const SID = "a".repeat(32);
const TID = "b".repeat(32);
const DIGEST = "c".repeat(64);

function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  const calls: unknown[][] = [];
  const order: string[] = [];
  const client = {
    fixReviewFinding: async (...a: unknown[]) => (calls.push(["fix", ...a]), order.push("command"), { findingId: a[2], inputId: "review-finding-fix:cccccccccccc:f1:0102", taskState: "Waiting(UserInput)", offset: 9n }),
    dismissReviewFinding: async (...a: unknown[]) => (calls.push(["dismiss", ...a]), order.push("command"), { findingId: a[2], dismissedBy: "user:u", reason: a[4], offset: 10n }),
  };
  registerReviewFindingsHandlers({
    handle: (c, fn) => void handlers.set(c, fn),
    client: () => client as never,
    sessionId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: session"); return v; },
    taskId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: task"); return v; },
    lease: async () => void order.push("lease"),
  });
  return { call: (c: string, ...a: unknown[]) => Promise.resolve().then(() => handlers.get(c)!(...a)), calls, order, channels: [...handlers.keys()] };
}

test("PX-214: one channel pair, and nothing else is registered for findings", () => {
  assert.deepEqual(harness().channels.sort(), ["review:dismissFinding", "review:fixFinding"]);
});

test("PX-214: malformed arguments are refused before anything reaches the Core", async () => {
  const h = harness();
  const refused = (what: string, c: string, ...a: unknown[]) => assert.rejects(h.call(c, ...a), /BAD_ARGUMENT/, what);
  await refused("a session id that is not hex", "review:fixFinding", "nope", TID, "f1", DIGEST);
  await refused("a task id that is a path", "review:fixFinding", SID, "../etc", "f1", DIGEST);
  await refused("a finding id with a slash", "review:fixFinding", SID, TID, "../f1", DIGEST);
  await refused("a finding id that is empty", "review:fixFinding", SID, TID, "", DIGEST);
  await refused("a finding id that is too long", "review:fixFinding", SID, TID, "f".repeat(49), DIGEST);
  await refused("a digest that is not sha256 hex", "review:fixFinding", SID, TID, "f1", "abc");
  await refused("no digest", "review:fixFinding", SID, TID, "f1");
  await refused("a dismissal with no reason", "review:dismissFinding", SID, TID, "f1", DIGEST);
  await refused("a dismissal with a blank reason", "review:dismissFinding", SID, TID, "f1", DIGEST, "   ");
  await refused("a dismissal with a reason over 500 characters", "review:dismissFinding", SID, TID, "f1", DIGEST, "x".repeat(501));
  await refused("a dismissal with a reason that is not text", "review:dismissFinding", SID, TID, "f1", DIGEST, 7);
  assert.deepEqual(h.calls, [], "nothing reached the Core");
  assert.deepEqual(h.order, [], "no lease was taken for a refused request");
});

test("PX-214: a fix and a dismissal take the session's lease, then forward one typed command and return plain data", async () => {
  const h = harness();
  const fix = (await h.call("review:fixFinding", SID, TID, "f1", DIGEST)) as Record<string, unknown>;
  assert.deepEqual(fix, { findingId: "f1", inputId: "review-finding-fix:cccccccccccc:f1:0102", taskState: "Waiting(UserInput)" });
  const dismissed = (await h.call("review:dismissFinding", SID, TID, "F3", DIGEST, "  not part of this change  ")) as Record<string, unknown>;
  assert.deepEqual(dismissed, { findingId: "F3", dismissedBy: "user:u", reason: "not part of this change" });
  assert.deepEqual(h.order, ["lease", "command", "lease", "command"]);
  assert.deepEqual(h.calls, [
    ["fix", SID, TID, "f1", DIGEST],
    ["dismiss", SID, TID, "F3", DIGEST, "not part of this change"],
  ]);
});

test("PX-214: validators", () => {
  assert.equal(requireFindingId("sec-1.a_b~2"), "sec-1.a_b~2");
  assert.equal(requireResultDigest(DIGEST), DIGEST);
  assert.equal(requireReason("  because  "), "because");
  assert.throws(() => requireFindingId("a b"), /BAD_ARGUMENT/);
  assert.throws(() => requireResultDigest("C".repeat(64)), /BAD_ARGUMENT/);
});
