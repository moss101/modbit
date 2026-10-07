import { test } from "node:test";
import assert from "node:assert/strict";
import { applyEvent, applyEvents, childrenOf, columns, emptyModel, fromSnapshot, type Event } from "./model.ts";

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
  // The wire spells the reason SCREAMING_SNAKE_CASE (`APPROVAL`, `USER_INPUT`); a snapshot spells it `Approval`.
  m = applyEvent(m, ev(4, "TaskWaiting", { reason: "APPROVAL" }), "t1");
  assert.equal(m.tasks.get("t1")!.waitReason, "Approval");
  assert.equal(m.tasks.get("t1")!.phase, "awaitingHuman");
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

test("FIX-21: Needs Attention is the Core's attention view; the card's own next action no longer decides it once the Core has answered", () => {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "asks" }), "a");
  m = applyEvent(m, ev(2, "TaskStarted"), "a");
  m = applyEvent(m, ev(3, "TaskWaiting", { reason: "USER_INPUT" }), "a");
  m = applyEvent(m, ev(4, "TaskCreated", { goal_text: "parent" }), "p");
  m = applyEvent(m, ev(5, "TaskStarted"), "p");
  // The reducer reads a failed child as the parent's next action…
  m = applyEvent(m, ev(6, "SubagentResultRecorded", { agent_id: "x", status: "FAILED", summary: "boom" }), "p");
  m = applyEvent(m, ev(7, "TaskCreated", { goal_text: "waits for capacity" }), "c");
  m = applyEvent(m, ev(8, "TaskQueued"), "c");
  m = applyEvent(m, ev(9, "TaskCreated", { goal_text: "done" }), "d");
  m = applyEvent(m, ev(10, "TaskCompleted"), "d");
  // Before the Core has answered: the card's next action stands in.
  assert.deepEqual(columns(m).needsAttention.map((c) => c.taskId).sort(), ["a", "p"]);
  // The Core answered with nothing for them: neither card claims attention.
  const none = columns(m, []);
  assert.equal(none.needsAttention.length, 0);
  assert.deepEqual(none.running.map((c) => c.taskId), ["p"]);
  assert.deepEqual(none.waiting.map((c) => c.taskId).sort(), ["a", "c"]);
  // The Core names a question on `a` and a capacity wait on the queued `c`, and (wrongly, but it is the Core's word) one on the finished `d`.
  const some = columns(m, [{ taskId: "a" }, { taskId: "c" }, { taskId: "d" }]);
  assert.deepEqual(some.needsAttention.map((c) => c.taskId).sort(), ["a", "c"]);
  assert.deepEqual(some.running.map((c) => c.taskId), ["p"]);
  assert.deepEqual(some.completed.map((c) => c.taskId), ["d"], "a finished task is never in Needs Attention");
});

/** n events over `cards` cards: creations first, then a mix that exercises every kind of write (a card, the agent map, links, a growing record list). */
function replay(n: number, cards: number, agentIds = Infinity): { event: Event; taskId: string }[] {
  const out: { event: Event; taskId: string }[] = [];
  let offset = 0;
  const push = (taskId: string, eventType: string, payload: unknown) => out.push({ event: { offset: String(++offset), sequence: "1", eventType, sessionId: "s", payload, occurredAtMs: offset }, taskId });
  for (let i = 0; i < cards; i++) push(`t${i}`, "TaskCreated", { goal_text: `task ${i}` });
  const kinds = ["TaskStarted", "TaskWaiting", "VerificationRunRecorded", "SecurityEventRecorded", "AgentNodeCreated", "AgentNodeTransitioned", "TaskResumed"];
  while (out.length < n) {
    const i = out.length;
    const kind = kinds[i % kinds.length]!;
    const payload = kind === "AgentNodeCreated" ? { node: { agent_id: `a${i % agentIds}`, kind: "SUBAGENT", status: "RUNNING" } } : kind === "AgentNodeTransitioned" ? { agent_id: `a${(i - 1) % agentIds}`, to: "COMPLETED" } : { reason: "APPROVAL", stage: "TARGETED", status: "PASSED", kind: "INJECTION", tool_name: "fs.read", patterns: ["p"], action: "marked" };
    push(`t${i % cards}`, kind, payload);
  }
  return out;
}

test("FIX-21: replaying 100k events is linear — in one batch and one event at a time — and the dedupe state stays a watermark", () => {
  // A session whose agents are few per task (as a fleet's are): both paths.
  const events = replay(100_000, 20, 140);
  const last = events[events.length - 1]!.event.offset;

  const t0 = performance.now();
  const batched = applyEvents(emptyModel(), events);
  const batchMs = performance.now() - t0;
  assert.equal(batched.cursor, last);
  assert.equal(batched.tasks.size, 20);

  // Agents never repeat: every task grows an ever larger agent set. Counting
  // is incremental and a batch writes its maps in place, so this is linear
  // too (it re-counted and re-copied every agent per event: minutes).
  const wide = replay(100_000, 20);
  const tw = performance.now();
  const wideModel = applyEvents(emptyModel(), wide);
  const wideMs = performance.now() - tw;
  assert.ok(wideMs < 5_000, `batched replay with unbounded agents took ${wideMs.toFixed(0)} ms`);
  const counted = [...wideModel.tasks.values()].reduce((n, c) => n + c.agents.total, 0);
  const held = [...wideModel.agents.values()].reduce((n, s) => n + s.size, 0);
  assert.equal(counted, held, "the incremental counts agree with the agents held");

  const t1 = performance.now();
  let one = emptyModel();
  for (const { event, taskId } of events) one = applyEvent(one, event, taskId);
  const oneMs = performance.now() - t1;
  assert.equal(one.cursor, last);
  // The two paths fold to the same cards.
  assert.deepEqual([...one.tasks.entries()], [...batched.tasks.entries()]);
  assert.deepEqual([...one.agents.entries()].map(([k, v]) => [k, [...v.entries()]]), [...batched.agents.entries()].map(([k, v]) => [k, [...v.entries()]]));

  // The old reducer copied a set of every offset ever seen per event: 100k
  // events took over half a minute and grew four-fold per doubling. Generous
  // absolute bounds (a loaded CI machine is several times slower than this
  // one) that quadratic behaviour cannot meet.
  assert.ok(batchMs < 5_000, `batched replay took ${batchMs.toFixed(0)} ms`);
  assert.ok(oneMs < 10_000, `one-at-a-time replay took ${oneMs.toFixed(0)} ms`);

  // Doubling the events roughly doubles the time (linear), it does not quadruple it.
  const half = replay(50_000, 20);
  const th = performance.now();
  let x = emptyModel();
  for (const { event, taskId } of half) x = applyEvent(x, event, taskId);
  const halfMs = performance.now() - th;
  if (halfMs > 100) assert.ok(oneMs < halfMs * 3.2, `100k took ${oneMs.toFixed(0)} ms against ${halfMs.toFixed(0)} ms for 50k`);

  // The dedupe state is the watermark: no field holds an offset per event.
  assert.deepEqual(Object.keys(one).sort(), ["agents", "cursor", "parents", "sessionId", "tasks"]);
  // A record list on a card is bounded, not a log.
  const security = [...one.tasks.values()].map((c) => c.security?.length ?? 0);
  assert.ok(Math.max(...security) <= 50, `security lists are bounded: ${security.join(",")}`);

  // A replay of everything (a reconnect from an older cursor) changes nothing, at once.
  const t2 = performance.now();
  assert.equal(applyEvents(one, events), one, "the same model, untouched");
  for (const { event, taskId } of events.slice(0, 1000)) assert.equal(applyEvent(one, event, taskId), one);
  assert.ok(performance.now() - t2 < 2_000);
});

test("FIX-21: an event at or below the watermark is a replay; the model it came from is never changed by folding the next one", () => {
  let m = emptyModel();
  m = applyEvent(m, ev(5, "TaskCreated", { goal_text: "g" }), "t");
  m = applyEvent(m, ev(6, "TaskStarted"), "t");
  const before = m;
  const cardBefore = m.tasks.get("t");
  const after = applyEvent(m, ev(7, "TaskWaiting", { reason: "APPROVAL" }), "t");
  // Copy on write: the held model is exactly as it was.
  assert.equal(before.tasks.get("t"), cardBefore);
  assert.equal(before.cursor, "6");
  assert.equal(after.tasks.get("t")!.state, "Waiting");
  // Replays: below, at, and (offsets compare as numbers, not text) 10 > 9.
  assert.equal(applyEvent(after, ev(7, "TaskStarted"), "t"), after);
  assert.equal(applyEvent(after, ev(3, "TaskStarted"), "t"), after);
  const ten = applyEvent(applyEvent(emptyModel(), ev(9, "TaskCreated", { goal_text: "g" }), "t"), ev(10, "TaskStarted"), "t");
  assert.equal(ten.tasks.get("t")!.state, "Running");
  assert.equal(ten.cursor, "10");
  // A batch that is wholly replay returns the model it was given.
  assert.equal(applyEvents(after, [{ event: ev(2, "TaskStarted"), taskId: "t" }]), after);
});
