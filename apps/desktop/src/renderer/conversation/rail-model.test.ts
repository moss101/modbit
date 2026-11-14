import { test } from "node:test";
import assert from "node:assert/strict";
import type { AccountingInfo, CheckpointInfo } from "../../shared/control-types.ts";
import type { ExtensionRowInfo } from "../../shared/customize-types.ts";
import type { TaskEconomicsSummary } from "../../preload/preload.ts";
import { backgroundOf, budgetLines, changeWords, checkpointHeading, checkpointLabel, folderWords, locationWords, modelCallWords, restorable, spendWords, toolServers, verificationWords } from "./rail-model.ts";

const economics = (over: Partial<TaskEconomicsSummary>): TaskEconomicsSummary => ({ state: "Running", verified: false, checksPassed: 0, checksFailed: 0, model: "m", modelCalls: 0, inputTokens: "0", cachedInputTokens: "0", outputTokens: "0", costUsd: 0, pricingKnown: false, toolCalls: 0, wallMs: "0", modelMs: "0", toolMs: "0", prefixCacheHits: 0, prefixCacheMisses: 0, compactionEpochs: 0, contextTokensInjected: "0", suggestionCalls: 0, ...over });
const checkpoint = (over: Partial<CheckpointInfo>): CheckpointInfo => ({ checkpointId: "c", epoch: 1, kind: "TURN", status: "CURRENT", reason: "", files: 0, removed: 0, eventOffset: "1", createdAtMs: 0, name: "", turnId: "t1", turnOrdinal: 1, retention: [], ...over });

test("PX-201: the location is the Core's word in the person's, and nothing is assumed before the header is read", () => {
  assert.equal(locationWords(undefined, "This Mac"), null, "no header: no location, not this machine by default");
  assert.equal(locationWords("", "This Mac"), null);
  assert.equal(locationWords("local", "This PC"), "This PC");
  assert.equal(locationWords("cloud", "This Mac"), "Isolated cloud");
  assert.equal(locationWords("edge", "This Mac"), "edge", "a word the desktop does not know is shown as written");
});

test("PX-201: a folder is its name and where it is, with either separator", () => {
  assert.deepEqual(folderWords("/Users/a/projects/modbit"), { name: "modbit", parent: "/Users/a/projects" });
  assert.deepEqual(folderWords("/Users/a/projects/modbit/"), { name: "modbit", parent: "/Users/a/projects" });
  assert.deepEqual(folderWords("C:\\work\\modbit"), { name: "modbit", parent: "C:\\work" });
  assert.deepEqual(folderWords("/modbit"), { name: "modbit", parent: "/" });
  assert.deepEqual(folderWords("modbit"), { name: "modbit", parent: "" });
  assert.equal(folderWords(""), null);
});

test("PX-201: the change is the header's counts, and no change is no size", () => {
  assert.deepEqual(changeWords({ filesChanged: 2, linesAdded: 20, linesRemoved: 1 }), { added: "+20", removed: "−1", files: "2 files" });
  assert.equal(changeWords({ filesChanged: 1, linesAdded: 0, linesRemoved: 0 })?.files, "1 file");
  assert.equal(changeWords({ filesChanged: 0, linesAdded: 0, linesRemoved: 0 }), null);
});

test("PX-201: verification says what the Core counted, and never 'passed' before a check ran", () => {
  assert.equal(verificationWords(null), null);
  assert.equal(verificationWords(economics({})), "not run yet");
  assert.equal(verificationWords(economics({ checksPassed: 2, checksFailed: 1 })), "2 of 3 passed");
  assert.equal(verificationWords(economics({ verified: true, checksPassed: 3 })), "verified, 3 of 3 passed");
});

test("PX-201: spend is priced only where the Core knows the price; the caps keep the Core's units", () => {
  assert.equal(spendWords(null), null);
  assert.equal(spendWords(economics({})), "not priced");
  assert.equal(spendWords(economics({ pricingKnown: true, costUsd: 0.0234 })), "$0.0234");
  assert.equal(spendWords(economics({ pricingKnown: true, costUsd: 12.5 })), "$12.50");
  assert.equal(modelCallWords(1), "1 model call");
  assert.equal(modelCallWords(9), "9 model calls");
  const accounting = (budgets: AccountingInfo["budgets"]): AccountingInfo => ({ available: true, totalTokens: "0", totalSource: "", windowTokens: "0", windowSource: "", usedBp: 0, estimatorErrorBp: 0, declaredErrorBp: 0, model: "", turnOrdinal: 0, categories: [], compactionEpoch: 0, compactionSummaries: 0, budgets });
  const b = { maxCostMinor: "1000", spentMinor: "85", heldByChildrenMinor: "0", maxWallMs: "3600000", wallMsUsed: "90000", maxChildren: 0, liveChildren: 0, forbidSpawn: false };
  assert.deepEqual(budgetLines(accounting(b)), [{ kind: "cost", text: "85 of 1,000 minor units", percent: 8 }, { kind: "time", text: "1 min 30 s of 60 min 0 s", percent: 2 }]);
  assert.deepEqual(budgetLines(accounting({ ...b, maxCostMinor: "0", maxWallMs: "0" })), [], "a cap of 0 is no cap");
  assert.deepEqual(budgetLines(accounting(null)), []);
  assert.deepEqual(budgetLines(null), []);
  assert.equal(budgetLines(accounting({ ...b, spentMinor: "5000" }))[0]!.percent, 100);
});

test("PX-201: tool servers are counted as the Core lists them, never as connected, and none is no section", () => {
  const ext = (servers: ExtensionRowInfo["servers"]): ExtensionRowInfo => ({ extensionId: "e", name: "e", version: "1", scope: "USER", state: "ACTIVE", quarantineReason: "", signature: "", manifestDigest: "", path: "", publisher: "", source: "", hooks: [], commands: [], providers: [], servers, capabilities: [], canTrust: false });
  const server = (state: string) => ({ name: "s", transport: "stdio", state, credentialRequired: false, credentialHeld: false, scopes: [] });
  assert.equal(toolServers(null), null);
  assert.equal(toolServers([]), null);
  assert.equal(toolServers([ext([])]), null);
  assert.deepEqual(toolServers([ext([server("CONFIGURED"), server("INERT")]), ext([server("CONFIGURED")])]), { configured: 2, inert: 1, words: "2 configured, 1 inert" });
  assert.equal(toolServers([ext([server("INERT")])])!.words, "1 inert");
  assert.doesNotMatch(toolServers([ext([server("CONFIGURED")])])!.words, /connected/);
});

test("PX-201: what runs can be stopped and what has ended is counted", () => {
  const bg = backgroundOf([{ state: "Running" }, { state: "Waiting" }, { state: "Completed" }, { state: "ReadyForReview" }, { state: "Cancelled" }, { state: "Failed" }, { state: "Queued" }], [{ state: "RUNNING" }, { state: "running" }, { state: "EXITED" }]);
  assert.deepEqual(bg.liveChildren.map((c) => c.state), ["Running", "Waiting", "Queued"]);
  assert.equal(bg.liveTerminals.length, 2);
  assert.equal(bg.finished, 5);
  assert.equal(backgroundOf([], []).finished, 0);
});

test("PX-201: the rail offers the committed turn boundaries, newest first, one per turn", () => {
  const list = { currentCheckpointId: "", currentEpoch: 4, checkpoints: [checkpoint({ checkpointId: "a", epoch: 1, turnId: "t1", turnOrdinal: 1, status: "SUPERSEDED" }), checkpoint({ checkpointId: "b", epoch: 2, turnId: "t2", turnOrdinal: 2, name: "before the edit" }), checkpoint({ checkpointId: "b2", epoch: 3, turnId: "t2", turnOrdinal: 2 }), checkpoint({ checkpointId: "p", epoch: 4, turnId: "", retention: ["PRE_RESTORE"] }), checkpoint({ checkpointId: "x", epoch: 5, turnId: "t3", status: "PENDING" })] };
  assert.deepEqual(restorable(list).map((c) => c.checkpointId), ["b2", "a"]);
  assert.deepEqual(restorable(null), []);
  assert.equal(checkpointLabel(list.checkpoints[1]!), "Turn 2 · before the edit");
  assert.equal(checkpointHeading(list.checkpoints[0]!), "at the end of turn 1");
  assert.equal(checkpointHeading(checkpoint({ turnOrdinal: 0 })), "at the end of this turn");
});
