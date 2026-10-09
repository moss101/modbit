import { test } from "node:test";
import assert from "node:assert/strict";
import { observeStreamEvent, openStreamsOf, registerConversationHandlers, validDensity } from "./conversation-ipc.ts";
import { TranscriptDensity } from "@modbit/surface-protocol";

const ev = (eventType: string, payload: unknown, id = "aa".repeat(16), taskId: string | null = "t".repeat(32)) => ({ eventType, aggregateType: "assistant_stream", aggregateId: id, taskId, payload });

test("main keeps the text of an open stream until its closing event, in order, without rewriting what it has", () => {
  const task = "1".repeat(32);
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: 1, text: "ab" }, "bb".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: 1, text: "ZZ" }, "bb".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: 3, text: "!!" }, "bb".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: 2, text: "cd" }, "bb".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", { kind: "REASONING", sequence: 3, text: "private" }, "bb".repeat(16), task));
  assert.deepEqual(openStreamsOf(task), [{ streamId: "bb".repeat(16), taskId: task, sequence: 2, text: "abcd" }]);
  assert.deepEqual(openStreamsOf("2".repeat(32)), []);
  observeStreamEvent(ev("AssistantMessageAborted", {}, "bb".repeat(16), task));
  assert.deepEqual(openStreamsOf(task), []);
});

test("main ignores what is not a stream event or is malformed", () => {
  const task = "3".repeat(32);
  observeStreamEvent({ ...ev("AssistantTextDelta", { kind: "TEXT", sequence: 1, text: "x" }, "cc".repeat(16), task), aggregateType: "task" });
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: "1", text: "x" }, "cc".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", null, "cc".repeat(16), task));
  observeStreamEvent(ev("AssistantTextDelta", { kind: "TEXT", sequence: 1, text: "x" }, "cc".repeat(16), null));
  assert.deepEqual(openStreamsOf(task), []);
});

test("the conversation handlers reject malformed arguments before anything reaches the Core", async () => {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  let reached = 0;
  const client = new Proxy({}, { get: () => () => { reached++; return Promise.reject(new Error("must not be reached")); } });
  registerConversationHandlers({
    handle: (c, fn) => void handlers.set(c, fn),
    client: () => client as never,
    sessionId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: session"); return v; },
    taskId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: task"); return v; },
    lease: async () => {},
  });
  const ok = "a".repeat(32);
  const bad: [string, unknown[]][] = [
    ["agents:headers", ["nope"]],
    ["agents:headers", [ok, "yes"]],
    ["conversation:transcript", ["x"]],
    ["conversation:transcript", [ok, { density: "HUGE" }]],
    ["conversation:transcript", [ok, { limit: 100000 }]],
    ["conversation:transcript", [ok, { afterRow: -1 }]],
    ["conversation:transcript", [ok, { asOfOffset: "12abc" }]],
    ["conversation:search", [ok, ""]],
    ["conversation:search", [ok, "x".repeat(300)]],
    ["conversation:search", [ok, "q", { taskId: "bad" }]],
    ["conversation:markRead", [ok, ok, "-1"]],
    ["conversation:archive", [ok, ok, "true"]],
    ["conversation:archive", ["bad", ok, true]],
  ];
  for (const [channel, args] of bad) await assert.rejects(async () => handlers.get(channel)!(...args), /BAD_ARGUMENT/, `${channel} ${JSON.stringify(args)}`);
  assert.equal(reached, 0);
  assert.equal(validDensity(undefined), TranscriptDensity.COMPACT);
  assert.equal(validDensity("DETAILED"), TranscriptDensity.DETAILED);
});
