import { test } from "node:test";
import assert from "node:assert/strict";
import { observeStreamEvent, openStreamsOf, planInfo, registerConversationHandlers, validDensity, workGraphInfo } from "./conversation-ipc.ts";
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
    ["conversation:workGraph", ["x"]],
    ["conversation:workGraph", [{ taskId: ok }]],
    ["conversation:plan", [undefined]],
    ["conversation:plan", [ok.toUpperCase()]],
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

test("PX-200: the work graph and the plan cross the bridge as the Core gave them: steps in order with their state, the version in force, nothing derived", async () => {
  const task = "c".repeat(32);
  const graph = workGraphInfo(task, {
    nodes: [
      { id: "find", title: "Find the signer", dependsOn: [], status: "DONE", expectedArtifacts: [], verification: "", evidenceRefs: ["test:1", "run:2"], blockers: [], attempts: 1, planVersion: 1 },
      { id: "route", title: "Route it", dependsOn: ["find"], ownerAgentId: { value: new Uint8Array(16).fill(7) }, status: "ACTIVE", expectedArtifacts: [], verification: "", evidenceRefs: [], blockers: [], attempts: 2, planVersion: 2 },
      { id: "later", title: "Verify", dependsOn: ["route"], status: "SOMETHING_NEW", expectedArtifacts: [], verification: "", evidenceRefs: [], blockers: ["needs a key"], attempts: 0, planVersion: 1 },
    ],
    ready: [],
    planVersion: 2,
  } as never);
  assert.deepEqual(graph.steps.map((s) => [s.id, s.status, s.owned, s.evidenceRefs, s.attempts]), [["find", "DONE", false, 2, 1], ["route", "ACTIVE", true, 0, 2], ["later", "SOMETHING_NEW", false, 0, 0]]);
  assert.deepEqual(graph.steps[2]!.blockers, ["needs a key"]);
  assert.equal(graph.planVersion, 2);
  const version = (n: number, outcome: string, stepsJson: string, provenance = "model") => ({ version: n, planRef: "", outcome, expectedFiles: [], verification: [], protectedEffects: [], stepsJson, provenance, reason: n > 1 ? "revised" : "", offset: BigInt(n), annotations: [] });
  const plan = planInfo(task, { currentVersion: 2, versions: [version(1, "first", '[{"id":"a"}]'), version(2, "second", "[]", "user_review")], executedVersions: [1, 1, 2] } as never);
  assert.deepEqual(plan, { taskId: task, currentVersion: 2, outcome: "second", provenance: "user_review", reason: "revised", versions: 2, stepsInVersion: 0, executedVersions: [1, 1, 2] });
  assert.equal(planInfo(task, { currentVersion: 1, versions: [version(1, "only", "not json")], executedVersions: [] } as never).stepsInVersion, 0);
  assert.deepEqual(planInfo(task, { currentVersion: 0, versions: [], executedVersions: [] } as never), { taskId: task, currentVersion: 0, outcome: "", provenance: "", reason: "", versions: 0, stepsInVersion: 0, executedVersions: [] });

  // The handlers forward the validated task id to the two typed reads and to nothing else.
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  const calls: string[] = [];
  const client = { getWorkGraph: async (t: string) => (calls.push(`graph:${t}`), { nodes: [], ready: [], planVersion: 0 }), getPlan: async (t: string) => (calls.push(`plan:${t}`), { currentVersion: 0, versions: [], executedVersions: [] }) };
  registerConversationHandlers({ handle: (c, fn) => void handlers.set(c, fn), client: () => client as never, sessionId: (v) => v as string, taskId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: task"); return v; }, lease: async () => { throw new Error("a read takes no lease"); } });
  assert.deepEqual(await handlers.get("conversation:workGraph")!(task), { taskId: task, steps: [], ready: [], planVersion: 0 });
  assert.equal(((await handlers.get("conversation:plan")!(task)) as { currentVersion: number }).currentVersion, 0);
  assert.deepEqual(calls, [`graph:${task}`, `plan:${task}`]);
});
