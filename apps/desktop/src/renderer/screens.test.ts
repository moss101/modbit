import { test } from "node:test";
import assert from "node:assert/strict";
import { applyEvent, emptyModel, type Event, type TaskCard } from "./model.ts";
import { fleetState, newTaskState, reviewState, settingsState, taskState, type AttentionLike, type CoreStatusLike } from "./screens.ts";

const ev = (offset: number, eventType: string, payload: unknown = {}, aggregate: { type?: string; id?: string } = {}): Event => ({ offset: String(offset), sequence: "1", eventType, sessionId: "s", payload, occurredAtMs: offset, ...(aggregate.type ? { aggregateType: aggregate.type } : {}), ...(aggregate.id ? { aggregateId: aggregate.id } : {}) });
const connected: CoreStatusLike = { state: "connected", restarts: 0 };

function cardAfter(events: [string, unknown, { type?: string; id?: string }?][]): TaskCard {
  let m = emptyModel();
  m = applyEvent(m, ev(1, "TaskCreated", { goal_text: "g" }), "t");
  let o = 2;
  for (const [type, payload, agg] of events) m = applyEvent(m, ev(o++, type, payload, agg), "t");
  return m.tasks.get("t")!;
}

test("PX-023: every screen has the six states and a degraded/error state names cause, next action and evidence", () => {
  // Fleet.
  assert.equal(fleetState({ core: { state: "starting", restarts: 0 }, loaded: "loading", error: null, recovery: null, recoveredBanner: false, tasks: [] }).kind, "loading");
  assert.equal(fleetState({ core: connected, loaded: "empty", error: null, recovery: null, recoveredBanner: false, tasks: [] }).kind, "empty");
  const running = cardAfter([["TaskStarted", {}]]);
  assert.equal(fleetState({ core: connected, loaded: "populated", error: null, recovery: null, recoveredBanner: false, tasks: [running] }).kind, "populated");
  const restarting = fleetState({ core: { state: "restarting", reason: "exit code 137", restarts: 1, retryInMs: 2000 }, loaded: "populated", error: null, recovery: null, recoveredBanner: false, tasks: [running] });
  assert.equal(restarting.kind, "degraded");
  assert.match(restarting.cause!, /exit code 137/);
  assert.match(restarting.nextAction!, /restarts in 2 s/);
  assert.ok(restarting.evidence);
  const staleBundle = fleetState({ core: connected, loaded: "populated", error: "REGISTRY_STALE: bundle 3 expired", recovery: null, recoveredBanner: false, tasks: [running] });
  assert.equal(staleBundle.kind, "degraded");
  assert.equal(staleBundle.label, "stale policy bundle");
  assert.match(staleBundle.nextAction!, /last-good bundle/);
  const failed = fleetState({ core: { state: "failed", reason: "gave up", restarts: 5 }, loaded: "populated", error: null, recovery: null, recoveredBanner: false, tasks: [] });
  assert.equal(failed.kind, "error");
  assert.match(failed.evidence!, /5 restart/);
  const recovered = fleetState({ core: connected, loaded: "populated", error: null, recovery: { bootGeneration: 3, eventsVerified: 41, aggregatesVerified: 4, sessions: 1, tasks: 2, projectionsRebuilt: true, notes: ["1 tool call unknown"], recoveryMs: 12 }, recoveredBanner: true, tasks: [running] });
  assert.equal(recovered.kind, "recovery");
  assert.match(recovered.cause!, /verified 41 event\(s\) across 4 aggregate\(s\)/, "recovery states what the Core verified, never progress");
  assert.match(recovered.nextAction!, /1 tool call unknown/);
  assert.equal(recovered.evidence, "recovery report of boot 3");
  const providerDown = cardAfter([["TaskStarted", {}], ["TaskWaiting", { reason: "Provider" }], ["TaskNeedsAttention", { reason: "provider", diagnostic: { class: "PROVIDER", code: "PROVIDER_UNAVAILABLE", detail: "connection refused", user_action: "check the provider", recovery_path: "resume", retryable: true, evidence_refs: ["event:4"] } }]]);
  const pd = fleetState({ core: connected, loaded: "populated", error: null, recovery: null, recoveredBanner: false, tasks: [providerDown] });
  assert.equal(pd.kind, "degraded");
  assert.equal(pd.label, "provider down");
  assert.match(pd.cause!, /PROVIDER_UNAVAILABLE: connection refused/);
  assert.equal(pd.nextAction, "check the provider");
  assert.match(pd.evidence!, /offset 4/);

  // New Task.
  assert.equal(newTaskState({ core: { state: "restarting", restarts: 1 }, provider: null, trusted: null, workspaceRoot: "", lastError: null }).kind, "loading");
  assert.equal(newTaskState({ core: connected, provider: { configured: true }, trusted: "/r", workspaceRoot: "/r", lastError: null }).kind, "populated");
  const untrusted = newTaskState({ core: connected, provider: { configured: true }, trusted: null, workspaceRoot: "/r", lastError: "REPOSITORY_UNTRUSTED: /r" });
  assert.equal(untrusted.kind, "error");
  assert.match(untrusted.nextAction!, /trust it/);
  const noProvider = newTaskState({ core: connected, provider: { configured: false }, trusted: "/r", workspaceRoot: "/r", lastError: null });
  assert.equal(noProvider.kind, "degraded");
  assert.match(noProvider.nextAction!, /will not start until a provider/);
  assert.equal(newTaskState({ core: connected, provider: { configured: true }, trusted: "/r", workspaceRoot: "/r", lastError: "PLAN_MODE_ONLY: policy" }).label, "policy forbids the requested execution mode");
  assert.equal(newTaskState({ core: connected, provider: { configured: true }, trusted: "/r", workspaceRoot: "/r", lastError: "BUDGET_EXHAUSTED" }).kind, "degraded");

  // Task.
  const attention: AttentionLike[] = [];
  assert.equal(taskState({ card: running, core: connected, attention }).kind, "populated");
  const reconnecting = taskState({ card: running, core: { state: "restarting", reason: "crash", restarts: 1 }, attention });
  assert.equal(reconnecting.label, "reconnecting by cursor");
  assert.match(reconnecting.evidence!, /cursor 2/);
  const approval = cardAfter([["TaskStarted", {}], ["TaskWaiting", { reason: "Approval" }], ["ApprovalRequested", { tool_name: "shell.exec", effect_class: "ProcessExecution", intent_hash: "abcdef0123456789" }, { type: "approval", id: "ap1" }]]);
  const ap = taskState({ card: approval, core: connected, attention });
  assert.equal(ap.label, "awaiting approval");
  assert.match(ap.nextAction!, /abcdef012345…/, "the exact intent the decision binds");
  assert.equal(ap.evidence, "approval ap1");
  const unknown = cardAfter([["TaskStarted", {}], ["TaskWaiting", { reason: "External" }], ["TaskNeedsAttention", { reason: "unknown outcome", diagnostic: { class: "UNKNOWN_OUTCOME", code: "RESTART_RECONCILING", detail: "shell.exec ended in an unknown state", user_action: "confirm or deny", recovery_path: "reconcile", retryable: false, evidence_refs: ["call:c1"] } }]]);
  const uo = taskState({ card: unknown, core: connected, attention });
  assert.equal(uo.label, "unknown tool outcome under reconciliation");
  assert.equal(uo.evidence, "call:c1");
  const infeasible = cardAfter([["TaskStarted", {}], ["RoutingPlanAdmitted", { feasibility: "QUALITY_FLOOR_INFEASIBLE" }, { type: "run", id: "r1" }]]);
  assert.equal(taskState({ card: infeasible, core: connected, attention }).label, "quality floor infeasible under policy");
  const question = cardAfter([["TaskStarted", {}], ["TaskWaiting", { reason: "UserInput" }], ["TaskNeedsAttention", { reason: "question" }]]);
  const q = taskState({ card: question, core: connected, attention: [{ kind: "QUESTION", taskId: "t", reference: "q1", reason: "which port?", action: "answer the question", sinceOffset: "4" }] });
  assert.equal(q.label, "awaiting your answer");
  assert.equal(q.evidence, "question q1");
  const failedCard = cardAfter([["TaskStarted", {}], ["TaskFailed", { reason: "x" }], ["TaskNeedsAttention", { reason: "failed", diagnostic: { class: "TOOL", code: "TOOL_FAILED", detail: "boom", user_action: "", recovery_path: "retry", retryable: true, evidence_refs: [] } }]]);
  const f = taskState({ card: failedCard, core: connected, attention });
  assert.equal(f.kind, "error");
  assert.match(f.cause!, /TOOL\/TOOL_FAILED: boom/);
  assert.match(f.evidence!, /offset 4/);

  // Review.
  assert.equal(reviewState({ loading: true, error: null, bundle: null, gate: null, pullRequest: null }).kind, "loading");
  const bundle = { taskState: "ReadyForReview", workspaceRevision: "7", files: [{}] };
  assert.equal(reviewState({ loading: false, error: null, bundle, gate: { verdict: "ACCEPT", missingEvidence: [], rejectReasons: [], gateRef: "g", humanRequired: false }, pullRequest: null }).kind, "populated");
  assert.equal(reviewState({ loading: false, error: null, bundle: { ...bundle, files: [] }, gate: null, pullRequest: null }).kind, "empty");
  const inconclusive = reviewState({ loading: false, error: null, bundle, gate: { verdict: "INCONCLUSIVE", missingEvidence: ["test run"], rejectReasons: [], gateRef: "gate0123456789ab", humanRequired: true }, pullRequest: null });
  assert.equal(inconclusive.label, "verification incomplete");
  assert.match(inconclusive.cause!, /missing: test run/);
  assert.equal(inconclusive.evidence, "gate gate01234567");
  const rejected = reviewState({ loading: false, error: null, bundle, gate: { verdict: "REJECT", missingEvidence: [], rejectReasons: ["tests fail"], gateRef: "g", humanRequired: false }, pullRequest: null });
  assert.equal(rejected.label, "acceptance rejected");
  assert.equal(rejected.cause, "tests fail");
  const stale = reviewState({ loading: false, error: "STALE_REVIEW: revision 7 is not the candidate", bundle, gate: null, pullRequest: null });
  assert.equal(stale.label, "stale candidate revision");
  const denied = reviewState({ loading: false, error: null, bundle, gate: null, pullRequest: { status: "DENIED", detail: "forge.pr.create denied", evidence: "approval ab12cd34" } });
  assert.equal(denied.label, "pull request push denied or failed");
  assert.equal(denied.evidence, "approval ab12cd34");

  // Settings.
  assert.equal(settingsState({ provider: null, organizationOverrides: [] }).kind, "loading");
  const noKeychain = settingsState({ provider: { keychainAvailable: false, configured: true }, organizationOverrides: [] });
  assert.equal(noKeychain.label, "keychain unavailable");
  assert.match(noKeychain.cause!, /held for this session only/);
  assert.equal(settingsState({ provider: { keychainAvailable: true, configured: true }, organizationOverrides: [{ key: "policy.mode", value: "plan", by: "org" }] }).label, "1 organization override(s) shown");
});

test("PX-023: TaskStarted/Resumed clear the diagnostic and the awaiting-human phase; the card's evidence offset follows the last event", () => {
  const resumed = cardAfter([["TaskStarted", {}], ["TaskWaiting", { reason: "Provider" }], ["TaskNeedsAttention", { reason: "provider", diagnostic: { class: "PROVIDER", code: "P", detail: "d", user_action: "", recovery_path: "", retryable: true, evidence_refs: [] } }], ["TaskResumed", {}]]);
  assert.equal(resumed.diagnostic, null);
  assert.equal(resumed.lastOffset, "5");
  assert.equal(taskState({ card: resumed, core: connected, attention: [] }).kind, "populated");
});
