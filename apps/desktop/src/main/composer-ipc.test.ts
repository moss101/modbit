import { test } from "node:test";
import assert from "node:assert/strict";
import { ProtocolError, RejectedError } from "@modbit/ide-adapter-core";
import { registerComposerHandlers, requireInputId, requireMode, requirePreferencePatch, requireSkillNames, requireText, slashView, suggestionRefusal } from "./composer-ipc.ts";

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
  await refused("a suggestion for a session id that is not hex", "composer:suggest", "nope", TID);
  await refused("a suggestion for a task id that is not hex", "composer:suggest", SID, "../etc");
  await refused("a suggestion with no task", "composer:suggest", SID);
  await refused("a task id that is not hex", "composer:queue", "nope");
  await refused("attach bytes with no bytes", "composer:attachBytes", SID, TID, "a.png", "not bytes", "");
  await refused("attach bytes over the cap", "composer:attachBytes", SID, TID, "a.png", new Uint8Array(3 * 1024 * 1024 + 2), "");
  await refused("an attachment the bytes show is not an image", "composer:attachBytes", SID, TID, "evil.png", new TextEncoder().encode("MZ\u0090\u0000"), "image/png");
  await refused("an attachment path that is not a string", "composer:attachPath", SID, TID, 42, "");
  assert.equal(h.reached(), 0, "nothing reached the Core or the file system");
});

test("every composer channel is registered", () => {
  const h = harness();
  for (const c of ["posture", "setMode", "setPreference", "variants", "slash", "queue", "queueInput", "editQueued", "removeQueued", "reorderQueued", "sendNow", "interrupt", "sendBehavior", "setSendBehavior", "sideQuestion", "suggest", "attachPath", "attachBytes"]) assert.ok(h.channels.includes(`composer:${c}`), c);
});

test("PX-210: a suggestion is one typed Core command under the session's lease; the Core's refusal is an answer, any other failure is an error", async () => {
  const calls: string[] = [];
  let answer: () => Promise<unknown> = () => Promise.reject(new Error("unset"));
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  registerComposerHandlers({
    handle: (c, fn) => void handlers.set(c, fn),
    client: () => ({ suggestNextPrompt: (sid: string, tid: string) => (calls.push(`suggest ${sid} ${tid}`), answer()) }) as never,
    sessionId: (v) => v as string,
    taskId: (v) => v as string,
    lease: async (_c, sid) => void calls.push(`lease ${sid}`),
    thumbnail: () => null,
    readFile: () => ({ name: "x", bytes: new Uint8Array(0) }),
  });
  const suggest = () => Promise.resolve().then(() => handlers.get("composer:suggest")!(SID, TID));

  answer = () => Promise.resolve({ text: "Run the tests again", taskOffset: 9007199254740993n, inputTokens: 400n, outputTokens: 9n, costKnown: true, costMinor: 2n, endpoint: "openai", model: "m", snapshotMessages: 8, usageOffset: 9007199254740995n });
  assert.deepEqual(await suggest(), { text: "Run the tests again", refusal: "", reason: "", taskOffset: "9007199254740993", costKnown: true, costMinor: "2" });
  assert.deepEqual(calls, [`lease ${SID}`, `suggest ${SID} ${TID}`], "the lease first, then the one command, named for its session");

  for (const code of ["TURN_RUNNING", "INPUT_QUEUED", "BUDGET_EXHAUSTED", "BUDGET_WOULD_EXCEED", "COST_UNMETERED", "SUGGESTIONS_OFF", "TASK_ENDED", "STALE_LEASE"]) {
    answer = () => Promise.reject(new RejectedError(code, "the Core's own words"));
    assert.deepEqual(await suggest(), { text: "", refusal: code, reason: "the Core's own words", taskOffset: "0", costKnown: false, costMinor: "0" }, code);
  }
  // A lost Core or a bug is not "no suggestion": it is reported as the failure it is.
  answer = () => Promise.reject(new ProtocolError("DISCONNECTED", "client closed"));
  await assert.rejects(suggest(), /DISCONNECTED/);
  answer = () => Promise.reject(new TypeError("boom"));
  await assert.rejects(suggest(), /boom/);
  assert.equal(suggestionRefusal(new Error("TURN_RUNNING: looks like one")), null, "only the client's rejection type is the Core's answer");
  assert.deepEqual(suggestionRefusal(new RejectedError("INPUT_QUEUED", "1 queued input(s) are the task's next message")), { code: "INPUT_QUEUED", reason: "1 queued input(s) are the task's next message" });
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
