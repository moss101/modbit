import { test } from "node:test";
import assert from "node:assert/strict";
import type { AccountingInfo } from "../../shared/control-types.ts";
import { budgetBars, categorySum, countFrom, nearLimit, percentUsed, ringLabel, ringLevel, sumsToTotal, trayPlan, usageVisible } from "./model.ts";
import { parseUsageSetting } from "./prefs.ts";

const acct = (over: Partial<AccountingInfo> = {}): AccountingInfo => ({
  available: true,
  totalTokens: "1000",
  totalSource: "PROVIDER_REPORTED",
  windowTokens: "10000",
  windowSource: "REGISTRY",
  usedBp: 1000,
  estimatorErrorBp: 0,
  declaredErrorBp: 1500,
  model: "m",
  turnOrdinal: 1,
  categories: [
    { category: "SYSTEM", tokens: "600", estimatedTokens: "590", shareBp: 6000, sources: [] },
    { category: "CONVERSATION", tokens: "400", estimatedTokens: "410", shareBp: 4000, sources: [] },
  ],
  compactionEpoch: 0,
  compactionSummaries: 0,
  budgets: null,
  ...over,
});

test("REQ-PX-060: the ring is unknown until the Core has a request and a window; then ok, warn and danger by share of the window", () => {
  assert.equal(ringLevel(null), "unknown");
  assert.equal(ringLevel(acct({ available: false })), "unknown");
  assert.equal(ringLevel(acct({ windowSource: "UNKNOWN", windowTokens: "0" })), "unknown");
  assert.equal(ringLevel(acct()), "ok");
  assert.equal(ringLevel(acct({ usedBp: 8000 })), "warn");
  assert.equal(ringLevel(acct({ usedBp: 9500 })), "danger");
  assert.equal(percentUsed(acct({ usedBp: 1234 })), 12);
  assert.equal(percentUsed(acct({ usedBp: 20000 })), 100);
  assert.match(ringLabel(acct()), /^10% of 10,000 tokens$/);
  assert.match(ringLabel(acct({ usedBp: 9700 })), /nearly full/);
  assert.equal(ringLabel(null), "no request yet");
});

test("REQ-PX-060: the nine-category breakdown is checked against the total the Core counted", () => {
  assert.equal(categorySum(acct()), 1000n);
  assert.equal(sumsToTotal(acct()), true);
  assert.equal(sumsToTotal(acct({ totalTokens: "1001" })), false);
});

test("REQ-PX-060: budgets show as bars when capped; near a limit is the window or a cap at 80% or more; the usage setting decides when the summary shows", () => {
  const b = { maxCostMinor: "100", spentMinor: "85", heldByChildrenMinor: "0", maxWallMs: "0", wallMsUsed: "5", maxChildren: 0, liveChildren: 0, forbidSpawn: false };
  const a = acct({ budgets: b });
  assert.equal(budgetBars(a).length, 1, "a cap of 0 is no cap");
  assert.equal(budgetBars(a)[0]!.percent, 85);
  assert.equal(nearLimit(a), true);
  assert.equal(nearLimit(acct()), false);
  assert.equal(nearLimit(acct({ usedBp: 8500 })), true);
  assert.equal(usageVisible("auto", false), false);
  assert.equal(usageVisible("auto", true), true);
  assert.equal(usageVisible("always", false), true);
  assert.equal(usageVisible("never", true), false);
  assert.equal(parseUsageSetting("bogus"), "auto");
  assert.equal(parseUsageSetting("never"), "never");
});

const base = { coreState: "connected" as const, everConnected: true, coreReason: "", diagnostic: null, taskId: "t" };

test("REQ-PX-060: trays are due by the Core's typed codes: the Core away, BUDGET_EXHAUSTED, MODEL_NOT_ALLOWED; a start-up is not offline", () => {
  assert.deepEqual(trayPlan(base), []);
  assert.deepEqual(trayPlan({ ...base, coreState: "starting", everConnected: false }), [], "the first start is not an outage");
  assert.equal(trayPlan({ ...base, coreState: "restarting", coreReason: "exit 9" })[0]!.kind, "offline");
  assert.equal(trayPlan({ ...base, coreState: "failed", everConnected: false })[0]!.kind, "offline");
  const budget = trayPlan({ ...base, diagnostic: { code: "BUDGET_EXHAUSTED", detail: "max_wall_ms: 5/1", userAction: "raise it" } });
  assert.equal(budget[0]!.kind, "budget");
  assert.equal(trayPlan({ ...base, diagnostic: { code: "MODEL_NOT_ALLOWED", detail: "", userAction: "pick another" } })[0]!.kind, "policy");
  assert.equal(trayPlan({ ...base, diagnostic: { code: "ROUTE_REFUSED", detail: "endpoint openai model gpt-5 is blocked by organization policy (block=*)", userAction: "" } })[0]!.kind, "policy");
  assert.deepEqual(trayPlan({ ...base, diagnostic: { code: "ROUTE_REFUSED", detail: "no provider is configured", userAction: "" } }), [], "a refused route that is not the organisation's is not a policy tray");
  assert.deepEqual(trayPlan({ ...base, diagnostic: { code: "PROVIDER_FAILED", detail: "", userAction: "" } }), []);
  assert.deepEqual(trayPlan({ ...base, taskId: null, diagnostic: { code: "BUDGET_EXHAUSTED", detail: "", userAction: "" } }), [], "no tray without a task to resume");
});

test("REQ-PX-060: a new budget cap is whole digits or nothing", () => {
  assert.equal(countFrom(" 42 "), "42");
  for (const t of ["", "-1", "1.5", "1e3", "abc", "1".repeat(16)]) assert.equal(countFrom(t), null);
});
