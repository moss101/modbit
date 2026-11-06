import { test } from "node:test";
import assert from "node:assert/strict";
import { fromSnapshot, type TaskCard } from "../model.ts";
import { rowFactsOf } from "./row-facts.ts";

const snapshotCard = (state: string): TaskCard => fromSnapshot({ sessionId: "s", state: "Active", generation: 1, lastOffset: "1", tasks: [{ taskId: "t", goalText: "g", state, generation: 1, createdAtMs: 1 }] }).tasks.get("t")!;
const diagnostic = { class: "PROVIDER", code: "CONNECT_FAILED", detail: "the provider did not answer", userAction: "check the endpoint", recoveryPath: "", retryable: true, evidenceRefs: [] };

test("a stopped task's next is the Core's diagnostic, ahead of its attention item", () => {
  const f = rowFactsOf({ ...snapshotCard("Failed"), diagnostic }, [{ kind: "FAILURE", taskId: "t", reference: "r", reason: "other words", action: "a", sinceOffset: "1" }]);
  assert.equal(f.next, "the provider did not answer");
});

test("a waiting task with no diagnostic says the attention item's reason, not the protocol command that clears it", () => {
  const f = rowFactsOf(snapshotCard("Waiting"), [{ kind: "APPROVAL", taskId: "t", reference: "r", reason: "shell.exec asks for a Destructive effect", action: "ResolveApproval r (approve or deny)", sinceOffset: "1" }]);
  assert.equal(f.next, "shell.exec asks for a Destructive effect");
});

test("another task's attention item says nothing here", () => {
  assert.equal(rowFactsOf(snapshotCard("Running"), [{ kind: "STALL", taskId: "other", reference: "r", reason: "stalled", action: "a", sinceOffset: "1" }]).next, "");
});

test("a card from a snapshot invents no phase: the default phase and a finished task's phase are not shown", () => {
  assert.equal(rowFactsOf(snapshotCard("Running"), []).phase, "");
  assert.equal(rowFactsOf({ ...snapshotCard("Completed"), phase: "verifying" }, []).phase, "");
  assert.equal(rowFactsOf({ ...snapshotCard("Running"), phase: "verifying" }, []).phase, "verifying");
});

test("ready for review is not repeated as a next action or a phase, but a human gate on it is said", () => {
  const review = { ...snapshotCard("ReadyForReview"), nextAction: "Review the result", phase: "reviewing" as const };
  assert.equal(rowFactsOf(review, []).next, "");
  assert.equal(rowFactsOf(review, []).phase, "");
  assert.equal(rowFactsOf({ ...review, phase: "awaitingHuman" }, []).phase, "awaiting human");
});

test("active agents are the running and the background ones of all the AgentGraph counted", () => {
  const f = rowFactsOf({ ...snapshotCard("Running"), agents: { total: 4, running: 1, background: 1, waiting: 1, done: 1, failed: 0 } }, []);
  assert.equal(f.totalAgents, 4);
  assert.equal(f.activeAgents, 2);
});
