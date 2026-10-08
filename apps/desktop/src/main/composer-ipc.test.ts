import { test } from "node:test";
import assert from "node:assert/strict";
import { registerComposerHandlers, requireInputId, requireMode, requirePreferencePatch, requireSkillNames, requireText, slashView } from "./composer-ipc.ts";

const SID = "a".repeat(32);
const TID = "b".repeat(32);

function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  let reached = 0;
  const client = new Proxy({}, { get: () => () => { reached++; return Promise.reject(new Error("must not be reached")); } });
  registerComposerHandlers({
    handle: (c, fn) => void handlers.set(c, fn),
    client: () => client as never,
    sessionId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: session"); return v; },
    taskId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: task"); return v; },
    lease: async () => {},
    thumbnail: () => null,
    readFile: () => { reached++; return { name: "x.png", bytes: new Uint8Array(0) }; },
  });
  return { call: (c: string, ...a: unknown[]) => Promise.resolve().then(() => handlers.get(c)!(...a)), reached: () => reached, channels: [...handlers.keys()] };
}

test("the composer handlers reject malformed arguments before anything reaches the Core", async () => {
  const h = harness();
  const refused = async (what: string, c: string, ...a: unknown[]) => await assert.rejects(h.call(c, ...a), /BAD_ARGUMENT|ATTACHMENT_REFUSED/, what);
  await refused("a mode that is not one", "composer:setMode", SID, TID, "ROOT");
  await refused("a mode as a number", "composer:setMode", SID, TID, 2);
  await refused("an empty preference", "composer:setPreference", SID, TID, {});
  await refused("an unknown objective", "composer:setPreference", SID, TID, { objective: "FASTEST" });
  await refused("an effort out of range", "composer:setPreference", SID, TID, { effort: "extreme" });
  await refused("a half pin", "composer:setPreference", SID, TID, { pin: { endpoint: "openai" } });
  await refused("a pin and a clear together", "composer:setPreference", SID, TID, { pin: { endpoint: "openai", model: "gpt-5" }, clearPin: true });
  await refused("an input id with a slash", "composer:removeQueued", SID, TID, "../etc");
  await refused("empty text", "composer:queueInput", SID, TID, "   ", "DEFAULT", "id1");
  await refused("huge text", "composer:queueInput", SID, TID, "x".repeat(20_001), "DEFAULT", "id1");
  await refused("an unknown input mode", "composer:queueInput", SID, TID, "hi", "INTERRUPT", "id1");
  await refused("an edit that changes nothing", "composer:editQueued", SID, TID, "id1", {});
  await refused("an edit to a mode that is not queueing", "composer:editQueued", SID, TID, "id1", { mode: "DEFAULT" });
  await refused("a send behaviour that does not exist", "composer:setSendBehavior", SID, TID, "ALWAYS", "");
  await refused("a send-now behaviour that does not exist", "composer:setSendBehavior", SID, TID, "", "KILL");
  await refused("a side question with no text", "composer:sideQuestion", TID, "");
  await refused("a task id that is not hex", "composer:queue", "nope");
  await refused("attach bytes with no bytes", "composer:attachBytes", SID, TID, "a.png", "not bytes", "");
  await refused("attach bytes over the cap", "composer:attachBytes", SID, TID, "a.png", new Uint8Array(3 * 1024 * 1024 + 2), "");
  await refused("an attachment the bytes show is not an image", "composer:attachBytes", SID, TID, "evil.png", new TextEncoder().encode("MZ\u0090\u0000"), "image/png");
  await refused("an attachment path that is not a string", "composer:attachPath", SID, TID, 42, "");
  assert.equal(h.reached(), 0, "nothing reached the Core or the file system");
});

test("every composer channel is registered", () => {
  const h = harness();
  for (const c of ["posture", "setMode", "setPreference", "variants", "slash", "queue", "queueInput", "editQueued", "removeQueued", "reorderQueued", "sendNow", "interrupt", "sendBehavior", "setSendBehavior", "sideQuestion", "attachPath", "attachBytes"]) assert.ok(h.channels.includes(`composer:${c}`), c);
});

test("argument validators accept what they should", () => {
  assert.equal(requireMode("PLAN"), "PLAN");
  assert.equal(requireInputId("in-1_2.3:4"), "in-1_2.3:4");
  assert.equal(requireText("hello"), "hello");
  assert.deepEqual(requirePreferencePatch({ objective: "COST", effort: "high", serviceTier: "default", pin: { endpoint: "openai", model: "gpt-5" } }), { objective: "COST", effort: "high", serviceTier: "default", pin: { endpoint: "openai", model: "gpt-5" } });
  assert.deepEqual(requirePreferencePatch({ clearPin: true }), { clearPin: true });
  assert.deepEqual(requireSkillNames(undefined), []);
  assert.deepEqual(requireSkillNames(["alpha", "core.guide"]), ["alpha", "core.guide"]);
  assert.throws(() => requireSkillNames(["a b"]), /BAD_ARGUMENT/);
  assert.throws(() => requireSkillNames(Array.from({ length: 9 }, () => "a")), /BAD_ARGUMENT/);
});

test("the slash inventory is passed on as data, in the Core's order, markup untouched", () => {
  const v = slashView([{ kind: "SKILL", id: "x", displayName: "x", description: "<b>bold</b>", scope: "USER", trust: "UNTRUSTED", trustDetail: "d", enabled: false, invocation: "BOTH", builtIn: false } as never], 0);
  assert.equal(v.entries[0]!.description, "<b>bold</b>");
  assert.equal(v.entries[0]!.enabled, false);
});
