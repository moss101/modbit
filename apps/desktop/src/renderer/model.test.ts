import { test } from "node:test";
import assert from "node:assert/strict";
import { applyEvent, childrenOf, columns, emptyModel, fromSnapshot, type Event } from "./model.ts";

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

test("M6.6: agents, phases, capacity waits and children come from Core events; a child nests under its parent", () => {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "parent", origin: "cli" }), "p");
  m = applyEvent(m, ev(2, "TaskStarted"), "p");
  m = applyEvent(m, ev(3, "AgentNodeCreated", { node: { agent_id: "a0", kind: "PRIMARY", status: "RUNNING" } }), "p");
  assert.equal(m.tasks.get("p")!.agents.total, 1);
  assert.equal(m.tasks.get("p")!.agents.running, 1);
  // The child's task is created by the fork before the admission record lands.
  m = applyEvent(m, ev(4, "TaskCreated", { goal_text: "child work", origin: "subagent" }), "c");
  m = applyEvent(m, ev(5, "TaskQueued"), "c");
  m = applyEvent(m, ev(6, "AgentNodeCreated", { node: { agent_id: "a1", kind: "SUBAGENT", status: "ADMITTED", child_task_id: "c" } }), "p");
  m = applyEvent(m, ev(7, "SubagentAdmitted", { agent_id: "a1", child_task_id: "c", idempotency_key: "child-a", mode: "BACKGROUND" }), "p");
  const parent = m.tasks.get("p")!;
  assert.equal(parent.phase, "delegating");
  assert.deepEqual(parent.children, ["c"]);
  assert.equal(m.tasks.get("c")!.parentTaskId, "p");
  assert.equal(parent.agents.total, 2);
  // The child is not a top-level card; it is under its parent.
  const cols = columns(m);
  assert.equal(cols.running.length + cols.waiting.length + cols.needsAttention.length, 1);
  assert.equal(childrenOf(m, "p").length, 1);
  assert.equal(childrenOf(m, "p")[0]!.goalText, "child work");
  m = applyEvent(m, ev(8, "AgentNodeTransitioned", { agent_id: "a1", from: "ADMITTED", to: "BACKGROUND" }), "p");
  assert.equal(m.tasks.get("p")!.agents.background, 1);
  // Run-aggregate evidence sets the phase and the latest evidence line.
  m = applyEvent(m, ev(9, "VerificationRunRecorded", { stage: "COMPLETION", status: "PASSED" }), "p");
  assert.equal(m.tasks.get("p")!.phase, "verifying");
  assert.equal(m.tasks.get("p")!.latestEvidence, "COMPLETION verification PASSED");
  m = applyEvent(m, ev(10, "AcceptanceGateEvaluated", { verdict: "REJECT", human_required: false }), "p");
  assert.equal(m.tasks.get("p")!.phase, "reviewing");
  m = applyEvent(m, ev(11, "ContinuationActivated", { endpoint: "openai", model: "gpt-5" }), "p");
  assert.equal(m.tasks.get("p")!.phase, "escalating");
  m = applyEvent(m, ev(12, "RealizedRiskDerived", { level: "HIGH" }), "p");
  assert.equal(m.tasks.get("p")!.risk, "HIGH");
  // A child that ended badly is the parent's next action: Needs Attention.
  m = applyEvent(m, ev(13, "SubagentResultRecorded", { agent_id: "a1", child_task_id: "c", status: "FAILED", summary: "budget exhausted" }), "p");
  m = applyEvent(m, ev(14, "AgentNodeTransitioned", { agent_id: "a1", from: "BACKGROUND", to: "FAILED" }), "p");
  assert.equal(columns(m).needsAttention.length, 1);
  assert.match(m.tasks.get("p")!.nextAction ?? "", /child agent needs attention/);
  assert.equal(m.tasks.get("p")!.agents.failed, 1);
  // Capacity: a refused ticket is a typed wait, cleared by the grant.
  m = applyEvent(m, ev(15, "TaskCreated", { goal_text: "later", origin: "cli" }), "q");
  m = applyEvent(m, ev(16, "TaskQueued"), "q");
  m = applyEvent(m, ev(17, "CapacityDenied", { dimension: "model_concurrency", needed: 1, available: 0 }), "q");
  assert.equal(m.tasks.get("q")!.phase, "waitingCapacity");
  assert.match(m.tasks.get("q")!.nextAction ?? "", /Waiting for capacity: model_concurrency 0\/1/);
  m = applyEvent(m, ev(18, "CapacityTicketGranted", { ticket_id: "t-2" }), "q");
  assert.equal(m.tasks.get("q")!.nextAction, null);
  // Awaiting a human is a phase of its own.
  m = applyEvent(m, ev(19, "TaskStarted"), "q");
  m = applyEvent(m, ev(20, "TaskWaiting", { reason: "Approval" }), "q");
  assert.equal(m.tasks.get("q")!.phase, "awaitingHuman");
});

test("M6.6: a child named by dashed UUID text in a payload links to the card keyed by its hex envelope id", () => {
  const hex = "01a097a36cdd7ef197652cd12fe221ef";
  const dashed = "01a097a3-6cdd-7ef1-9765-2cd12fe221ef";
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "parent", origin: "desktop" }), "p");
  m = applyEvent(m, ev(2, "TaskCreated", { goal_text: "child work", origin: "subagent" }), hex);
  m = applyEvent(m, ev(3, "AgentNodeCreated", { node: { agent_id: "a1", kind: "SUBAGENT", status: "ADMITTED", child_task_id: dashed } }), "p");
  m = applyEvent(m, ev(4, "SubagentAdmitted", { agent_id: "a1", child_task_id: dashed, idempotency_key: "child-a", mode: "BACKGROUND" }), "p");
  assert.deepEqual(m.tasks.get("p")!.children, [hex]);
  assert.equal(childrenOf(m, "p").length, 1);
  assert.equal(m.tasks.get(hex)!.parentTaskId, "p");
  assert.equal(columns(m).waiting.length + columns(m).running.length + columns(m).needsAttention.length + columns(m).readyForReview.length + columns(m).completed.length, 1, "the child is never a top-level card");
});

test("REQ-EV-0046: a child's protected effect puts the running parent in Needs Attention with the evidence line", () => {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "parent", origin: "desktop" }), "p");
  m = applyEvent(m, ev(2, "TaskStarted"), "p");
  m = applyEvent(m, ev(3, "SubagentProtectedEffect", { agent_id: "a1", child_task_id: "c", idempotency_key: "child-a", tool: "git.worktree.close", effect_class: "DESTRUCTIVE", ceiling: "REVERSIBLE_WRITE" }), "p");
  assert.equal(m.tasks.get("p")!.latestEvidence, "child child-a reached a protected effect: git.worktree.close (DESTRUCTIVE)");
  assert.equal(columns(m).running.length, 1);
  m = applyEvent(m, ev(4, "TaskNeedsAttention", { reason: "subagent child-a reached a protected effect: decide" }), "p");
  assert.equal(columns(m).needsAttention.length, 1);
  assert.equal(columns(m).needsAttention[0]!.nextAction, "subagent child-a reached a protected effect: decide");
});
