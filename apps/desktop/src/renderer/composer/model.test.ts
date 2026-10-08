import { test } from "node:test";
import assert from "node:assert/strict";
import type { ModelCatalogView, PostureView, QueuedInputItem, SlashInventoryView } from "../../shared/composer-types.ts";
import {
  alternateLabel,
  applyTrigger,
  blockedTray,
  coreError,
  educationDue,
  elapsedLabel,
  EMPTY_STORE,
  enterAction,
  filePaths,
  HISTORY_START,
  liveMentions,
  loadStore,
  modeStatus,
  modelChip,
  nextMode,
  parseStore,
  pickerRows,
  placeholderFor,
  planSend,
  queueRows,
  reorderTarget,
  runningTerminals,
  saveStore,
  sendNowLabel,
  slashMenu,
  suggestionsFor,
  triggerAt,
  walkHistory,
  withDraft,
  withHistory,
  type KV,
  type KeyLike,
} from "./model.ts";

const key = (k: string, mods: Partial<KeyLike> = {}): KeyLike => ({ key: k, shiftKey: false, altKey: false, ctrlKey: false, metaKey: false, ...mods });

test("Shift+Tab walks Plan, Debug, Multitask, Ask and returns to the default; backwards is the reverse", () => {
  let m = nextMode("AGENT");
  const seen = [m];
  for (let i = 0; i < 4; i++) seen.push((m = nextMode(m)));
  assert.deepEqual(seen, ["PLAN", "DEBUG", "MULTITASK", "ASK", "AGENT"]);
  assert.equal(nextMode("AGENT", true), "ASK");
  assert.equal(nextMode("PLAN", true), "AGENT");
});

test("a mode the Core has not acknowledged is unconfirmed, never active; a mode ahead of the kernel is pending its boundary", () => {
  const posture = { mode: "AGENT", modeInForce: "AGENT" } as const;
  assert.equal(modeStatus(null, posture), "confirmed");
  assert.equal(modeStatus("PLAN", posture), "unconfirmed");
  assert.equal(modeStatus("PLAN", null), "unconfirmed");
  assert.equal(modeStatus("PLAN", { mode: "PLAN", modeInForce: "AGENT" }), "pending-boundary");
  assert.equal(modeStatus("PLAN", { mode: "PLAN", modeInForce: "PLAN" }), "confirmed");
});

test("placeholders are context specific and the suggestion chip of the active mode is hidden", () => {
  assert.match(placeholderFor({ mode: "AGENT", state: "Queued", running: false, hasMessages: false }), /Describe what you want done/);
  assert.match(placeholderFor({ mode: "AGENT", state: "ReadyForReview", running: false, hasMessages: true }), /Follow up/);
  assert.match(placeholderFor({ mode: "ASK", state: "Queued", running: false, hasMessages: true }), /question/);
  assert.match(placeholderFor({ mode: "PLAN", state: "Running", running: true, hasMessages: true }), /queues it/);
  assert.deepEqual(suggestionsFor("AGENT").map((s) => s.mode), ["PLAN", "MULTITASK"]);
  assert.deepEqual(suggestionsFor("PLAN").map((s) => s.mode), ["MULTITASK"]);
});

test("Enter sends, Shift+Enter is a newline, the primary modifier is the alternate, an IME composition is left alone", () => {
  assert.equal(enterAction(key("Enter"), true), "send");
  assert.equal(enterAction(key("Enter", { shiftKey: true }), true), "newline");
  assert.equal(enterAction(key("Enter", { metaKey: true }), true), "send-alternate");
  assert.equal(enterAction(key("Enter", { ctrlKey: true }), true), "none", "Ctrl is not the primary modifier on macOS");
  assert.equal(enterAction(key("Enter", { ctrlKey: true }), false), "send-alternate");
  assert.equal(enterAction(key("Enter", { isComposing: true }), true), "none");
  assert.equal(enterAction(key("a"), true), "none");
});

test("while a turn runs plain Enter is the Core's default and the alternate is the other of steer and queue", () => {
  const run = (w: string) => ({ running: true, startable: false, behavior: { whileRunning: w } });
  assert.deepEqual(planSend(run("QUEUE"), false), { mode: "DEFAULT", start: false, queues: true });
  assert.deepEqual(planSend(run("STEER"), false), { mode: "DEFAULT", start: false, queues: false });
  assert.deepEqual(planSend(run("QUEUE"), true), { mode: "STEER", start: false, queues: false });
  assert.deepEqual(planSend(run("STEER"), true), { mode: "FOLLOW_UP", start: false, queues: true });
  assert.deepEqual(planSend({ running: false, startable: true, behavior: null }, false), { mode: "FOLLOW_UP", start: true, queues: false });
  // The alternate's label never promises sending without stopping the agent.
  assert.doesNotMatch(alternateLabel({ whileRunning: "QUEUE" }), /without stopping/i);
  assert.match(alternateLabel({ whileRunning: "QUEUE" }), /cut the current model call/);
});

test("the education tray is due only for a send that queues, once", () => {
  assert.equal(educationDue({ educationAck: false }, true), true);
  assert.equal(educationDue({ educationAck: false }, false), false);
  assert.equal(educationDue({ educationAck: true }, true), false);
});

test("@ opens the mention menu after whitespace; / opens the slash menu only as the whole first line; a path in prose opens neither", () => {
  assert.deepEqual(triggerAt("look at @src/a", 14), { kind: "mention", query: "src/a", start: 8, end: 14 });
  assert.deepEqual(triggerAt("@", 1), { kind: "mention", query: "", start: 0, end: 1 });
  assert.equal(triggerAt("mail me@example.com", 19), null, "an @ inside a word is not a mention");
  assert.deepEqual(triggerAt("/rev", 4), { kind: "slash", query: "rev", start: 0, end: 4 });
  assert.equal(triggerAt("see /usr/bin", 12), null);
  assert.equal(triggerAt("/usr/bin", 8), null, "a path is not a command");
  assert.equal(triggerAt("line one\n/rev", 13), null);
  const t = triggerAt("look at @sr", 11)!;
  assert.deepEqual(applyTrigger("look at @sr tail", t, "@src/a.ts "), { text: "look at @src/a.ts  tail", caret: 18 });
});

test("a mention chip lives only while its @text is in the message", () => {
  const ms = [
    { kind: "file", value: "src/a.ts", label: "src/a.ts" },
    { kind: "file", value: "b.ts", label: "b.ts" },
    { kind: "terminal", value: "terminal:t1", label: "dev server" },
  ] as const;
  const live = liveMentions("read @src/a.ts and @terminal:t1", ms);
  assert.deepEqual(live.map((m) => m.value), ["src/a.ts", "terminal:t1"]);
  assert.deepEqual(filePaths(live), ["src/a.ts"]);
  assert.deepEqual(liveMentions("x@src/a.ts", ms), [], "an @ glued to a word is not a mention");
  assert.deepEqual(liveMentions("read @src/a.tsx", ms), [], "a longer path is not the shorter mention");
});

const inventory: SlashInventoryView = {
  dividerAt: 2,
  entries: [
    { kind: "SKILL", id: "core-guide", displayName: "core-guide", description: "how the product works", scope: "SYSTEM", trust: "SYSTEM", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: true },
    { kind: "SKILL", id: "review", displayName: "review", description: "<img src=x onerror=alert(1)>", scope: "SYSTEM", trust: "SYSTEM", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: true },
    { kind: "SKILL", id: "alpha", displayName: "alpha", description: "trusted user skill", scope: "USER", trust: "TRUSTED_BY_OWNER", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: false },
    { kind: "SKILL", id: "zeta", displayName: "zeta", description: "nobody vouches", scope: "USER", trust: "UNTRUSTED", trustDetail: "not trusted: the owner has not reviewed this content", enabled: false, invocation: "USER_ONLY", builtIn: false },
    { kind: "COMMAND", id: "kit/tidy", displayName: "kit/tidy", description: "", scope: "EXTENSION", trust: "SIGNED", trustDetail: "", enabled: true, invocation: "USER_ONLY", builtIn: false },
  ],
};

test("the slash menu keeps the Core's order and divider, filters by what is typed, and keeps an untrusted entry with its reason", () => {
  const all = slashMenu(inventory, "", { running: false });
  assert.deepEqual(all.items.map((i) => i.entry.id), ["core-guide", "review", "alpha", "zeta", "kit/tidy"]);
  assert.equal(all.dividerAt, 2);
  assert.equal(all.items[2]!.disabledReason, null);
  assert.equal(all.items[3]!.disabledReason, "not trusted: the owner has not reviewed this content");
  assert.match(all.items[4]!.disabledReason ?? "", /not available yet/);
  const f = slashMenu(inventory, "al", { running: false });
  assert.deepEqual(f.items.map((i) => i.entry.id), ["alpha"]);
  assert.equal(f.dividerAt, 0, "the divider shows only between built-in and imported entries");
  const during = slashMenu(inventory, "alpha", { running: true });
  assert.match(during.items[0]!.disabledReason ?? "", /once this turn ends/);
});

test("drafts and history round-trip, are bounded, and a broken store is an empty one", () => {
  const mem = new Map<string, string>();
  const kv: KV = { getItem: (k) => mem.get(k) ?? null, setItem: (k, v) => void mem.set(k, v) };
  let s = loadStore(kv);
  assert.deepEqual(s, EMPTY_STORE);
  s = withDraft(s, "t1", { text: "half a thought", mentions: [{ kind: "file", value: "a.ts", label: "a.ts" }], skill: "alpha" });
  s = withHistory(s, "t1", "first");
  s = withHistory(s, "t1", "first");
  s = withHistory(s, "t1", "  second  ");
  saveStore(kv, { ...s, educationAck: true });
  const back = loadStore(kv);
  assert.equal(back.drafts.t1?.text, "half a thought");
  assert.equal(back.drafts.t1?.skill, "alpha");
  assert.deepEqual(back.history.t1, ["first", "second"]);
  assert.equal(back.educationAck, true);
  assert.deepEqual(withDraft(back, "t1", { text: "", mentions: [], skill: null }).drafts, {}, "an emptied draft is removed");
  for (let i = 0; i < 60; i++) s = withDraft(s, `task-${i}`, { text: `d${i}`, mentions: [], skill: null });
  assert.equal(Object.keys(s.drafts).length, 40);
  assert.ok(!("t1" in s.drafts), "the oldest drafts fall off");
  assert.deepEqual(parseStore("{not json"), EMPTY_STORE);
  assert.deepEqual(parseStore('{"drafts":{"t":{"text":5}},"history":{"t":[1,2]}}'), EMPTY_STORE);
  const throwing: KV = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("quota"); } };
  assert.deepEqual(loadStore(throwing), EMPTY_STORE);
  assert.doesNotThrow(() => saveStore(throwing, s));
});

test("Alt+Up and Alt+Down walk the history and return to the unsent text", () => {
  const list = ["one", "two", "three"];
  let r = walkHistory(list, HISTORY_START, "older", "draft")!;
  assert.equal(r.text, "three");
  r = walkHistory(list, r.cursor, "older", r.text)!;
  assert.equal(r.text, "two");
  r = walkHistory(list, r.cursor, "older", r.text)!;
  assert.equal(r.text, "one");
  assert.equal(walkHistory(list, r.cursor, "older", r.text), null, "nothing before the oldest");
  r = walkHistory(list, r.cursor, "newer", r.text)!;
  assert.equal(r.text, "two");
  r = walkHistory(list, r.cursor, "newer", r.text)!;
  r = walkHistory(list, r.cursor, "newer", r.text)!;
  assert.equal(r.text, "draft", "past the newest comes the draft that was being written");
  assert.equal(walkHistory(list, r.cursor, "newer", r.text), null);
  assert.equal(walkHistory([], HISTORY_START, "older", ""), null);
});

const item = (id: string, position: number, text: string, extra: Partial<QueuedInputItem> = {}): QueuedInputItem => ({ inputId: id, position, mode: "FOLLOW_UP", text, state: "QUEUED", model: "", edited: false, sentNow: false, provenance: "", untrusted: false, ...extra });

test("the queue tray shows only queued items in the Core's order, with truthful mode words", () => {
  const rows = queueRows([item("b", 2, "second\n  line"), item("a", 1, "first"), item("x", 0, "gone", { state: "DISPATCHED" }), item("c", 3, "z".repeat(300), { mode: "STEER", edited: true })]);
  assert.deepEqual(rows.map((r) => r.inputId), ["a", "b", "c"]);
  assert.equal(rows[1]!.line, "second line");
  assert.ok(rows[2]!.line.length <= 120 && rows[2]!.line.endsWith("…"));
  assert.match(rows[2]!.modeLabel, /safe point/);
  assert.match(rows[0]!.modeLabel, /after this one/);
  assert.deepEqual(rows.map((r) => [r.canMoveUp, r.canMoveDown]), [[false, true], [true, true], [true, false]]);
});

test("reordering names the input to move before, and the end as empty", () => {
  const rows = queueRows([item("a", 1, "a"), item("b", 2, "b"), item("c", 3, "c")]);
  assert.deepEqual(reorderTarget(rows, "c", "up"), { inputId: "c", before: "b" });
  assert.deepEqual(reorderTarget(rows, "a", "down"), { inputId: "a", before: "c" });
  assert.deepEqual(reorderTarget(rows, "b", "down"), { inputId: "b", before: "" });
  assert.equal(reorderTarget(rows, "a", "up"), null);
  assert.equal(reorderTarget(rows, "c", "down"), null);
  assert.equal(reorderTarget(rows, "zz", "up"), null);
});

test("Send now says what it does, in the Core's words when it has them", () => {
  assert.match(sendNowLabel({ sendNow: "INTERRUPT", sendNowConsequence: "" }).label, /interrupts/);
  assert.match(sendNowLabel({ sendNow: "STEER", sendNowConsequence: "" }).title, /may cut/);
  assert.equal(sendNowLabel({ sendNow: "INTERRUPT", sendNowConsequence: "the Core's sentence" }).title, "the Core's sentence");
  assert.doesNotMatch(sendNowLabel(null).label + sendNowLabel(null).title, /without stopping/i);
});

test("the terminals chip counts running terminals and the timer is steady text", () => {
  assert.deepEqual(runningTerminals([{ state: "running" }, { state: "exited" }, { state: "Running" }]).length, 2);
  assert.equal(elapsedLabel(0, 4_000), "4s");
  assert.equal(elapsedLabel(0, 125_000), "2m 05s");
  assert.equal(elapsedLabel(0, 3_725_000), "1h 02m");
  assert.equal(elapsedLabel(10_000, 5_000), "0s", "a clock that ran backwards shows zero");
});

const posture = (over: Partial<PostureView["preference"]> = {}, routing: Partial<PostureView["routing"]> = {}): PostureView => ({
  mode: "AGENT",
  modeInForce: "AGENT",
  modeOffset: "0",
  writes: true,
  effectCeiling: "none",
  subagents: false,
  reproductionFirst: false,
  preference: { objective: "BALANCE", effort: "", serviceTier: "", pinEndpoint: "", pinModel: "", offset: "0", appliedOffset: "0", effortApplied: "", serviceTierApplied: "", ...over },
  routing: { outcome: "DIRECT", reasonCode: "NO_ACTIVE_REGISTRY", detail: "", floorMode: "", ...routing },
});

const catalog: ModelCatalogView = {
  objectives: ["COST", "BALANCE", "INTELLIGENCE"],
  defaultObjective: "BALANCE",
  models: [
    { endpoint: "openai", model: "gpt-5", provider: "openai", reasoning: true, vision: true, contextTokens: 400000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "", pinRefusalCode: "", pinAllowed: true, variants: [
      { effort: "low", serviceTier: "", label: "Low effort", isDefault: false, raisesCost: false },
      { effort: "medium", serviceTier: "", label: "Medium effort", isDefault: true, raisesCost: false },
      { effort: "high", serviceTier: "", label: "High effort", isDefault: false, raisesCost: true },
    ] },
    { endpoint: "openai", model: "gpt-5-mini", provider: "openai", reasoning: true, vision: true, contextTokens: 400000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "block=openai/gpt-5-mini", pinRefusalCode: "POLICY_BLOCKED", pinAllowed: false, variants: [{ effort: "medium", serviceTier: "", label: "Medium effort", isDefault: true, raisesCost: false }] },
    { endpoint: "openai", model: "gpt-4.1", provider: "openai", reasoning: false, vision: true, contextTokens: 1000000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "", pinRefusalCode: "", pinAllowed: true, variants: [{ effort: "", serviceTier: "", label: "Standard", isDefault: true, raisesCost: false }] },
  ],
};

test("the model chip says Auto with the objective until a pin is recorded, and routing is stated as the Core states it", () => {
  assert.deepEqual(modelChip(posture(), catalog), { name: "Auto", variant: "Balance", routing: "direct: no active registry" });
  assert.deepEqual(modelChip(posture({ objective: "COST" }), catalog).variant, "Cost");
  assert.deepEqual(modelChip(posture({ pinEndpoint: "openai", pinModel: "gpt-5", effort: "high" }), catalog), { name: "gpt-5", variant: "high effort", routing: "direct: no active registry" });
  assert.equal(modelChip(null, null).name, "Model");
});

test("the picker lists one row per variant, marks the current pin, and shows a blocked model with the Core's typed reason", () => {
  const rows = pickerRows(catalog, posture({ pinEndpoint: "openai", pinModel: "gpt-5", effort: "high" }), "");
  assert.deepEqual(rows.map((r) => r.id), ["openai/gpt-5#low", "openai/gpt-5#medium", "openai/gpt-5#high", "openai/gpt-5-mini#medium", "openai/gpt-4.1#standard"]);
  assert.deepEqual(rows.filter((r) => r.selected).map((r) => r.id), ["openai/gpt-5#high"]);
  assert.deepEqual(rows.filter((r) => r.raisesCost).map((r) => r.id), ["openai/gpt-5#high"]);
  const blocked = rows.find((r) => r.model === "gpt-5-mini")!;
  assert.deepEqual(blocked.blocked, { code: "POLICY_BLOCKED", reason: "block=openai/gpt-5-mini" });
  assert.equal(rows.find((r) => r.model === "gpt-5")!.blocked, null);
  assert.deepEqual(pickerRows(catalog, posture(), "4.1").map((r) => r.model), ["gpt-4.1"]);
  assert.deepEqual(pickerRows(null, posture(), ""), []);
  // With no pin nothing is selected: the objective row is the current choice.
  assert.equal(pickerRows(catalog, posture(), "").some((r) => r.selected), false);
  const t = blockedTray("gpt-5-mini", blocked.blocked!);
  assert.equal(t.code, "POLICY_BLOCKED");
  assert.match(t.title, /gpt-5-mini/);
});

test("a typed refusal is read out of the IPC error text", () => {
  assert.deepEqual(coreError(new Error("Error invoking remote method 'composer:setPreference': Error: POLICY_BLOCKED: openai/gpt-5-mini is blocked")), { code: "POLICY_BLOCKED", message: "openai/gpt-5-mini is blocked" });
  assert.deepEqual(coreError(new Error("something plain")), { code: "", message: "something plain" });
});
