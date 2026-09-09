import { test } from "node:test";
import assert from "node:assert/strict";
import { applyEvent, columns, emptyModel, fromSnapshot, type Event } from "./model.ts";

const ev = (offset: number, eventType: string, payload: unknown = {}): Event => ({ offset: String(offset), sequence: "1", eventType, sessionId: "s", payload, occurredAtMs: offset });

test("cards only reach Completed through a TaskCompleted event and events are idempotent by offset", () => {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "g" }), "t1");
  m = applyEvent(m, ev(2, "TaskQueued"), "t1");
  assert.equal(columns(m).waiting.length, 1);
  m = applyEvent(m, ev(2, "TaskQueued"), "t1");
  assert.equal(m.tasks.get("t1")!.generation, 2, "duplicate offset ignored");
  m = applyEvent(m, ev(3, "TaskStarted"), "t1");
  assert.equal(columns(m).running.length, 1);
  m = applyEvent(m, ev(4, "TaskWaiting", { reason: "Approval" }), "t1");
  assert.equal(columns(m).needsAttention[0]!.nextAction, "Approve or deny the pending effect");
  m = applyEvent(m, ev(5, "TaskResumed"), "t1");
  m = applyEvent(m, ev(6, "TaskReadyForReview"), "t1");
  assert.equal(columns(m).readyForReview.length, 1);
  assert.equal(columns(m).completed.length, 0);
  m = applyEvent(m, ev(7, "TaskCompleted"), "t1");
  assert.equal(columns(m).completed.length, 1);
  assert.equal(m.cursor, "7");
});

test("snapshot seeds the model with normalized waiting states", () => {
  const m = fromSnapshot({ sessionId: "s", state: "Active", generation: 1, lastOffset: "9", tasks: [{ taskId: "a", goalText: "x", state: "Waiting(Provider)", generation: 3, createdAtMs: 1 }, { taskId: "b", goalText: "y", state: "Queued", generation: 2, createdAtMs: 2 }] });
  const c = columns(m);
  assert.equal(c.waiting.length, 2);
  assert.equal(m.tasks.get("a")!.waitReason, "Provider");
  assert.equal(m.cursor, "9");
});
