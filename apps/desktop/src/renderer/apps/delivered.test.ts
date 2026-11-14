import { test } from "node:test";
import assert from "node:assert/strict";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import { createDeliveredStore, MAX_EVENTS_PER_TASK, MAX_TASKS } from "./delivered.ts";

const ev = (offset: number, over: Record<string, unknown> = {}) => ({ offset: String(offset), eventType: "TaskStarted", aggregateType: "task", taskId: "t1", occurredAtMs: 1000 + offset, payload: {}, ...over });

test("REQ-PX-202: the delivered buffer keeps the events pushed for a task, in the log's order, once each", () => {
  const s = createDeliveredStore();
  let changes = 0;
  const off = s.subscribe(() => changes++);
  assert.equal(s.event(ev(5)), true);
  assert.equal(s.event(ev(7, { eventType: "ApprovalRequested", aggregateType: "approval" })), true);
  assert.equal(s.event(ev(5)), false, "a redelivery adds nothing");
  assert.equal(s.event(ev(6)), true, "an event delivered late takes its place in the order");
  assert.deepEqual(s.get("t1").events.map((e) => e.offset), ["5", "6", "7"]);
  assert.equal(changes, 3);
  assert.deepEqual(s.get("other").events, []);
  off();
  s.event(ev(8));
  assert.equal(changes, 3);
});

test("REQ-PX-202: what is not a task's event is not kept, and a piece of a message is not an entry", () => {
  const s = createDeliveredStore();
  assert.equal(s.event(ev(1, { taskId: null })), false);
  assert.equal(s.event(ev(2, { taskId: "" })), false);
  assert.equal(s.event(ev(3, { offset: "x" })), false);
  assert.equal(s.event(ev(4, { eventType: 7 })), false);
  assert.equal(s.event(null), false);
  assert.equal(s.event("TaskStarted"), false);
  assert.equal(s.event(ev(5, { eventType: "AssistantTextDelta", aggregateType: "assistant_stream" })), false);
  assert.equal(s.event(ev(6, { eventType: "AssistantMessageCompleted", aggregateType: "assistant_stream" })), true);
  assert.deepEqual(s.get("t1").events.map((e) => e.eventType), ["AssistantMessageCompleted"]);
});

test("REQ-PX-202: the buffer is bounded, per task and in tasks, and holds the conversation's rows as given", () => {
  const s = createDeliveredStore();
  for (let i = 1; i <= MAX_EVENTS_PER_TASK + 10; i++) s.event(ev(i));
  const kept = s.get("t1").events;
  assert.equal(kept.length, MAX_EVENTS_PER_TASK);
  assert.equal(kept[0]!.offset, "11", "the oldest fall out first");
  const rows = [{ rowId: "r1" }] as unknown as TranscriptRowView[];
  s.transcript("t1", rows, "42");
  assert.equal(s.get("t1").rows, rows);
  assert.equal(s.get("t1").asOfOffset, "42");
  assert.equal(s.get("t1").events.length, MAX_EVENTS_PER_TASK, "the rows do not replace the events");
  const before = s.get("t1");
  s.transcript("t1", rows, "42");
  assert.equal(s.get("t1"), before, "the same rows again change nothing (a stable snapshot)");
  for (let i = 0; i < MAX_TASKS; i++) s.event(ev(1, { taskId: `task-${i}` }));
  assert.deepEqual(s.get("t1").events, [], "the task untouched for longest falls out once there are too many");
});
