import { test } from "node:test";
import assert from "node:assert/strict";
import { applyEvent, emptyModel, type Event, type TaskCard } from "./model.ts";
import { BURST_THRESHOLD, DEFAULT_PREFERENCES, deriveNotifications, newSince, osDeliveryDue } from "./notifications.ts";
import type { AttentionLike } from "./screens.ts";

const ev = (offset: number, eventType: string, payload: unknown = {}): Event => ({ offset: String(offset), sequence: "1", eventType, sessionId: "s", payload, occurredAtMs: offset });

function card(id: string, events: [string, unknown][]): TaskCard {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: `goal ${id}` }), id);
  let o = 2;
  for (const [type, payload] of events) m = applyEvent(m, ev(o++, type, payload), id);
  return m.tasks.get(id)!;
}
const item = (taskId: string, kind: string, reason: string, action: string, reference = "ref"): AttentionLike => ({ kind, taskId, reference, reason, action, sinceOffset: "1" });

test("PX-023: notifications exist for attention, completion and failure only; a task's reasons coalesce; each deep-links", () => {
  const running = card("r", [["TaskStarted", {}]]);
  const ready = card("v", [["TaskStarted", {}], ["TaskReadyForReview", {}]]);
  const failed = card("f", [["TaskStarted", {}], ["TaskFailed", { reason: "x" }], ["TaskNeedsAttention", { reason: "failed", diagnostic: { class: "TOOL", code: "T", detail: "boom", user_action: "retry", recovery_path: "", retryable: true, evidence_refs: [] } }]]);
  const waiting = card("w", [["TaskStarted", {}], ["TaskWaiting", { reason: "Approval" }]]);
  const attention = [item("w", "APPROVAL", "shell.exec wants to run", "approve or deny"), item("w", "QUESTION", "which port?", "answer")];
  const n = deriveNotifications([running, ready, failed, waiting], attention);
  assert.deepEqual(n.map((x) => x.id).sort(), ["task:f", "task:v", "task:w"], "routine progress (a running task) never notifies");
  const w = n.find((x) => x.id === "task:w")!;
  assert.equal(w.kind, "attention");
  assert.equal(w.coalesced, 2, "two reasons on one task collapse into one notification");
  assert.equal(w.nextAction, "approve or deny", "one next action: the oldest");
  assert.match(w.reason, /APPROVAL: shell.exec wants to run · QUESTION: which port\?/);
  assert.deepEqual(w.deepLink, { screen: "task", taskId: "w" });
  const v = n.find((x) => x.id === "task:v")!;
  assert.equal(v.kind, "completion");
  assert.deepEqual(v.deepLink, { screen: "review", taskId: "v" });
  const f = n.find((x) => x.id === "task:f")!;
  assert.equal(f.kind, "failure");
  assert.match(f.reason, /TOOL\/T: boom/);
  assert.equal(f.nextAction, "retry");
});

test("PX-023: a fleet-wide burst collapses into one count that links to the attention list", () => {
  const tasks: TaskCard[] = [];
  const attention: AttentionLike[] = [];
  for (let i = 0; i < BURST_THRESHOLD + 2; i++) {
    tasks.push(card(`t${i}`, [["TaskStarted", {}], ["TaskWaiting", { reason: "Approval" }]]));
    attention.push(item(`t${i}`, "APPROVAL", "r", "a"));
  }
  const n = deriveNotifications(tasks, attention);
  assert.equal(n.length, 1);
  assert.equal(n[0]!.id, "burst");
  assert.equal(n[0]!.count, BURST_THRESHOLD + 2);
  assert.equal(n[0]!.title, `${BURST_THRESHOLD + 2} tasks need you`);
  assert.deepEqual(n[0]!.deepLink, { screen: "fleet", taskId: null });
});

test("PX-023: OS delivery is opt-in per kind and silent in quiet hours; only new notifications are delivered", () => {
  const n = deriveNotifications([card("v", [["TaskStarted", {}], ["TaskReadyForReview", {}]])], [])[0]!;
  assert.equal(osDeliveryDue(n, DEFAULT_PREFERENCES, 12), false, "off until opted in");
  const on = { os: { attention: false, completion: true, failure: false }, quietHours: null };
  assert.equal(osDeliveryDue(n, on, 12), true);
  assert.equal(osDeliveryDue(n, { ...on, quietHours: { start: 22, end: 7 } }, 23), false, "quiet hours across midnight");
  assert.equal(osDeliveryDue(n, { ...on, quietHours: { start: 22, end: 7 } }, 3), false);
  assert.equal(osDeliveryDue(n, { ...on, quietHours: { start: 22, end: 7 } }, 9), true);
  assert.equal(osDeliveryDue(n, { ...on, quietHours: { start: 9, end: 17 } }, 12), false);
  assert.deepEqual(newSince([], [n]).map((x) => x.id), ["task:v"]);
  assert.deepEqual(newSince([n], [n]), [], "unchanged: not delivered twice");
  assert.deepEqual(newSince([n], [{ ...n, reason: "changed" }]).map((x) => x.id), ["task:v"], "a changed reason is new");
});
