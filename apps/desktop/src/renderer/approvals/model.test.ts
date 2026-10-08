import { test } from "node:test";
import assert from "node:assert/strict";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { alwaysAvailable, cardKey, commandLine, defaultPrefix, deckOrder, humanTitle, isIrreversible, prefixLadder, refusalWords, stepIndex, surfacedIndex, whyAsked, type KeyFacts } from "./model.ts";

const approval = (over: Partial<DockApprovalView> = {}): DockApprovalView => ({
  approvalId: "a1",
  taskId: "t1",
  toolCallId: "c1",
  toolName: "shell.exec",
  effectClass: "ProtectedWrite",
  intentHash: "h".repeat(64),
  requestedAtMs: 100,
  expiresAtMs: 0,
  reason: { code: "NOT_IN_ALLOWLIST", runMode: "ALLOWLIST", askClasses: [], contained: false, capabilities: ["shell.exec"], executionProfile: "local" },
  intent: { found: true, argv: ["mytool", "build"], command: "", cwd: "", paths: [], hosts: [], escalation: "", argsPreview: "{}", truncated: false },
  ...over,
});

test("REQ-PX-058: the typed reason is worded from the Core's code; an unknown code is said to be unknown", () => {
  assert.match(whyAsked(approval().reason).headline, /No allowlist rule/);
  assert.match(whyAsked({ ...approval().reason, code: "MODE_ASK" }).headline, /Ask mode/);
  assert.match(whyAsked({ ...approval().reason, code: "ALWAYS_ASK:NETWORK" }).headline, /Network always ask/);
  assert.match(whyAsked({ ...approval().reason, code: "ALWAYS_ASK:PROTECTED_PATH" }).detail, /every run mode/);
  assert.match(whyAsked({ ...approval().reason, code: "" }).headline, /did not give a reason/);
});

test("REQ-PX-058: the command line keeps token boundaries visible; the prefix ladder grows one word at a time and the default is the exact command", () => {
  assert.equal(commandLine(["git", "commit", "-m", "a b"]), 'git commit -m "a b"');
  assert.deepEqual(prefixLadder(["a", "b", "c"]), [["a"], ["a", "b"], ["a", "b", "c"]]);
  assert.deepEqual(defaultPrefix(["a", "b", "c"]), ["a", "b", "c"]);
  assert.equal(prefixLadder(Array.from({ length: 20 }, (_, i) => `t${i}`)).length, 8);
});

test("REQ-PX-058: Always is offered only for a command with an argv that is not an always-ask or irreversible effect", () => {
  assert.equal(alwaysAvailable(approval()).ok, true);
  assert.equal(alwaysAvailable(approval({ intent: { ...approval().intent, argv: null } })).ok, false);
  assert.equal(alwaysAvailable(approval({ reason: { ...approval().reason, askClasses: ["NETWORK"], code: "ALWAYS_ASK:NETWORK" } })).ok, false);
  assert.equal(alwaysAvailable(approval({ effectClass: "Destructive" })).ok, false);
  assert.equal(isIrreversible("Destructive"), true);
  assert.equal(isIrreversible("ExternalSideEffect"), true);
  assert.equal(isIrreversible("ProtectedWrite"), false);
});

test("REQ-PX-058: the deck surfaces the oldest first, keeps the card the person was on, and steps round", () => {
  const list = deckOrder([approval({ approvalId: "c", requestedAtMs: 300 }), approval({ approvalId: "a", requestedAtMs: 100 }), approval({ approvalId: "b", requestedAtMs: 200 })]);
  assert.deepEqual(list.map((a) => a.approvalId), ["a", "b", "c"]);
  assert.equal(surfacedIndex(list, null), 0);
  assert.equal(surfacedIndex(list, "b"), 1);
  assert.equal(surfacedIndex(list, "gone"), 0, "a decided card is gone: the oldest surfaces");
  assert.equal(stepIndex(2, 3, 1), 0);
  assert.equal(stepIndex(0, 3, -1), 2);
  assert.equal(stepIndex(0, 1, 1), 0);
});

const k = (over: Partial<KeyFacts>): KeyFacts => ({ key: "Enter", shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, focus: "neutral", inCard: false, confirming: false, alwaysOffered: true, ...over });

test("REQ-PX-058: Enter runs, Shift+Enter allows the prefix, Escape skips; typing elsewhere decides nothing; a focused button keeps its own Enter", () => {
  assert.equal(cardKey(k({})), "run");
  assert.equal(cardKey(k({ shiftKey: true })), "always");
  assert.equal(cardKey(k({ shiftKey: true, alwaysOffered: false })), null);
  assert.equal(cardKey(k({ key: "Escape" })), "skip");
  assert.equal(cardKey(k({ focus: "editable" })), null, "a person typing a message cannot approve by accident");
  assert.equal(cardKey(k({ focus: "editable", key: "Escape" })), null);
  assert.equal(cardKey(k({ focus: "editable", inCard: true, key: "Escape" })), "skip");
  assert.equal(cardKey(k({ focus: "control", inCard: true })), null, "Enter on the Skip button is the button's");
  assert.equal(cardKey(k({ ctrlKey: true })), null);
  assert.equal(cardKey(k({ metaKey: true, key: "Escape" })), null);
  assert.equal(cardKey(k({ key: "a" })), null);
});

test("REQ-PX-058: the open task's approvals surface before the other agents', and a foreign card is never decided by a bare key", () => {
  const list = deckOrder([approval({ approvalId: "x", taskId: "other", requestedAtMs: 10 }), approval({ approvalId: "b", requestedAtMs: 300 }), approval({ approvalId: "a", requestedAtMs: 200 })], "t1");
  assert.deepEqual(list.map((a) => a.approvalId), ["a", "b", "x"]);
  assert.equal(cardKey(k({ foreign: true })), null);
  assert.equal(cardKey(k({ foreign: true, key: "Escape" })), null);
});

test("REQ-PX-058: an irreversible effect asks twice: Enter confirms, Escape goes back, and Shift+Enter does nothing", () => {
  assert.equal(cardKey(k({ confirming: true })), "confirm");
  assert.equal(cardKey(k({ confirming: true, key: "Escape" })), "back");
  assert.equal(cardKey(k({ confirming: true, shiftKey: true })), null);
});

test("REQ-PX-058: titles are readable and a refusal is explained in the Core's terms", () => {
  assert.equal(humanTitle("shell.exec"), "Run a command");
  assert.equal(humanTitle("custom.tool_name"), "Custom tool name");
  assert.match(refusalWords("Error invoking remote method 'x': Error: INTENT_MISMATCH: no"), /no longer the one this card showed/);
  assert.match(refusalWords("STALE_APPROVAL: x"), /no longer waiting/);
});
