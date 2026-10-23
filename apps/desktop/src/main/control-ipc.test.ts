import { test } from "node:test";
import assert from "node:assert/strict";
import { registerControlHandlers } from "./control-ipc.ts";
import type { Registrar } from "./conversation-ipc.ts";

const SID = "0123456789abcdef0123456789abcdef";
const TID = "fedcba9876543210fedcba9876543210";
const H = "a".repeat(64);

/** A registrar whose Core client explodes when touched: a handler that reaches it has not finished validating. */
function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  let touched = 0;
  const r: Registrar = {
    handle: (channel, fn) => void handlers.set(channel, fn),
    client: () => {
      touched++;
      throw new Error("CLIENT_TOUCHED");
    },
    sessionId: (v) => {
      if (v !== SID) throw new Error("BAD_ARGUMENT: session id must be 32 hex chars");
      return SID;
    },
    taskId: (v) => {
      if (v !== TID) throw new Error("BAD_ARGUMENT: task id must be 32 hex chars");
      return TID;
    },
    lease: () => Promise.resolve(),
  };
  registerControlHandlers(r);
  const call = async (channel: string, ...args: unknown[]): Promise<string> => {
    const fn = handlers.get(channel);
    assert.ok(fn, `${channel} is registered`);
    try {
      await fn(...args);
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  };
  return { call, touched: () => touched, channels: [...handlers.keys()] };
}

test("REQ-PX-057/058/060/062: every control channel is registered, and invalid arguments are refused before the Core client is touched", async () => {
  const h = harness();
  for (const c of ["approvals:dock", "approval:allowAlways", "runmode:get", "runmode:set", "rules:list", "rules:add", "rules:revoke", "accounting:get", "budgets:set", "checkpoints:list", "checkpoints:preview", "checkpoints:restore", "checkpoints:fork"]) assert.ok(h.channels.includes(c), c);
  const rule = { pattern: ["git", "status"], scope: "REPO", expiresAtMs: 0, coversAlwaysAsk: false };
  const bad: [string, unknown[]][] = [
    ["approvals:dock", ["nope"]],
    ["approval:allowAlways", [SID, "nope", H, rule]],
    ["approval:allowAlways", [SID, SID, "short", rule]],
    ["approval:allowAlways", [SID, SID, H, { ...rule, scope: "GLOBAL" }]],
    ["runmode:get", ["nope"]],
    ["runmode:set", [SID, TID, "AUTO_REVIEW", true]],
    ["runmode:set", [SID, TID, "ASK", "yes"]],
    ["rules:list", [TID, "yes"]],
    ["rules:add", [SID, TID, { ...rule, pattern: [] }]],
    ["rules:revoke", [SID, TID, ""]],
    ["accounting:get", ["nope"]],
    ["budgets:set", [SID, TID, { maxWallMs: "-1" }]],
    ["checkpoints:list", ["nope"]],
    ["checkpoints:preview", [TID, { checkpointId: "x" }]],
    ["checkpoints:restore", [SID, TID, { turnOrdinal: 1, name: "a" }, {}]],
    ["checkpoints:restore", [SID, TID, {}, { expectedCurrentEpoch: -1 }]],
    ["checkpoints:restore", [SID, TID, {}, { redo: true }]],
    ["checkpoints:restore", [SID, TID, {}, { keepPaths: ["/etc/passwd"] }]],
    ["checkpoints:fork", [SID, TID, { name: "a" }, ""]],
    ["checkpoints:fork", [SID, TID, {}, "g", "short"]],
  ];
  for (const [channel, args] of bad) assert.match(await h.call(channel, ...args), /BAD_ARGUMENT/, `${channel} ${JSON.stringify(args)}`);
  assert.equal(h.touched(), 0, "no invalid request reached the Core client");
  // A valid request does reach it (the harness's client then fails, which proves the call got that far).
  assert.equal(await h.call("runmode:get", TID), "CLIENT_TOUCHED");
  assert.equal(await h.call("rules:add", SID, TID, rule), "CLIENT_TOUCHED");
  assert.equal(await h.call("checkpoints:restore", SID, TID, { turnOrdinal: 2 }, { keepPaths: ["src/a.ts"], expectedCurrentEpoch: 3 }), "CLIENT_TOUCHED");
});
