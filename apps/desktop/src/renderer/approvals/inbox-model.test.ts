/**
 * REQ-PX-206: Inbox's counts and words. One count, the rows Needs you draws, is the badge and the tab (the owner's review of
 * 2026-11-17); it is the Core's: a store that says a task needs attention while the Core lists nothing counts for nothing, and the
 * task count the badge used to show is not it (retained mutations). A destructive effect
 * is refused by Inbox's decision before anything reaches the bridge, as on home.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { columns, emptyModel, type Model, type TaskCard } from "../model.ts";
import { decideFromList, decideProposal } from "./decide.ts";
import { askWords, effectTag, needsYouCount, needsYouOthers, proposalQuestion, UPDATE_WORDS, whenWords } from "./inbox-model.ts";

const A = "a".repeat(32);
const B = "b".repeat(32);

// The count the badge showed before the owner's review: the tasks the Core lists an item for. Kept here as the retained mutation.
const taskCount = (attention: readonly { taskId: string }[]): number => new Set(attention.map((i) => i.taskId)).size;

test("one count for the badge and the tab: the rows Needs you draws (approvals once, the Core's other items, pending proposals)", () => {
  const attention = [
    { kind: "APPROVAL", taskId: A, reference: "ap1" },
    { kind: "APPROVAL", taskId: B, reference: "ap2" },
    { kind: "STALL", taskId: A, reference: "s1" },
    { kind: "QUESTION", taskId: B, reference: "q1" },
  ];
  const approvals = [{ approvalId: "ap1" }, { approvalId: "ap2" }];
  const proposals = [{ proposalId: "p1" }];
  // Rows: two approval cards, the stall and the question (an approval on the same task does not hide them), one proposal.
  const rows = approvals.length + needsYouOthers(attention).length + proposals.length;
  assert.deepEqual(needsYouOthers(attention).map((i) => i.kind), ["STALL", "QUESTION"]);
  assert.equal(needsYouCount(attention, approvals, proposals), rows);
  assert.equal(rows, 5);
  // Deciding one lowers it by one.
  assert.equal(needsYouCount(attention.slice(1), approvals.slice(1), proposals), 4);
  assert.equal(needsYouCount([], [], []), 0);
});

test("retained mutations: the task count, and a count from the renderer's store, are not Needs you's count", () => {
  const attention = [
    { kind: "APPROVAL", taskId: A, reference: "ap1" },
    { kind: "APPROVAL", taskId: B, reference: "ap2" },
  ];
  // Two approvals and a proposal: Needs you draws three rows; the task count says two (the 3 against 4 the owner saw).
  assert.equal(needsYouCount(attention, [{ approvalId: "ap1" }, { approvalId: "ap2" }], [{ proposalId: "p" }]), 3);
  assert.equal(taskCount(attention), 2);
  // The store holds a task whose last event left a pending approval card; the Core has since resolved it and lists nothing.
  const card = { taskId: A, parentTaskId: null, origin: "desktop", createdAtMs: 1, state: "Waiting", approval: { approvalId: "x", toolName: "shell.exec", effectClass: "ProtectedWrite", intentHash: "h" }, question: null, nextAction: null } as unknown as TaskCard;
  const model: Model = { ...emptyModel(), tasks: new Map([[A, card]]) };
  assert.equal(columns(model).needsAttention.length, 1, "the store's column says the task needs attention");
  assert.equal(needsYouCount([], [], []), 0, "the Core lists nothing");
});

test("plain words: no tool name, effect class or Core command reaches the card's words", () => {
  const a = { toolName: "shell.exec", intent: { argv: ["git", "clean", "-fd"], command: "" } } as unknown as DockApprovalView;
  assert.equal(askWords(a), "Run this command?");
  assert.equal(askWords({ toolName: "git.push", intent: { argv: [], command: "" } } as unknown as DockApprovalView), "Push commits?");
  for (const c of ["ReadOnly", "ReversibleWrite", "ProtectedWrite", "ExternalSideEffect", "SecretAccess", "Destructive", "SomethingNew"]) assert.doesNotMatch(effectTag(c), /_|[a-z][A-Z]|Destructive|External/, c);
  assert.equal(effectTag("Destructive"), "Cannot be undone");
  assert.equal(proposalQuestion({ toMode: "PLAN" }), "Switch to Plan mode?");
  for (const w of Object.values(UPDATE_WORDS)) assert.doesNotMatch(w.title, /_/);
});

test("times read as the board's: now, minutes, hours today, yesterday, then a date", () => {
  const now = new Date(2026, 10, 16, 15, 0).getTime();
  assert.equal(whenWords(now - 10_000, now), "now");
  assert.equal(whenWords(now - 14 * 60_000, now), "14m");
  assert.equal(whenWords(now - 6 * 3_600_000, now), "6h");
  assert.equal(whenWords(new Date(2026, 10, 15, 9, 0).getTime(), now), "Yesterday");
  assert.notEqual(whenWords(new Date(2026, 10, 1).getTime(), now), "Yesterday");
});

test("Inbox's decision refuses a destructive effect before the bridge; a protected write goes through with Inbox named", async () => {
  const calls: unknown[][] = [];
  (globalThis as unknown as { window: unknown }).window = {
    modbit: {
      resolveApproval: async (...a: unknown[]) => {
        calls.push(a);
        return { status: "APPROVED" };
      },
      decideModeProposal: async (...a: unknown[]) => {
        calls.push(["proposal", ...a]);
        return {};
      },
    },
  };
  const base = { approvalId: "ap", taskId: A, toolName: "shell.exec", intentHash: "hash", computer: null } as unknown as DockApprovalView;
  for (const approve of [true, false]) await assert.rejects(decideFromList("s", { ...base, effectClass: "Destructive" } as DockApprovalView, approve, "inbox"), /NOT_FROM_A_LIST/);
  assert.equal(calls.length, 0, "nothing reached the bridge");
  await decideFromList("s", { ...base, effectClass: "ProtectedWrite" } as DockApprovalView, true, "inbox");
  assert.deepEqual(calls[0], ["s", "ap", true, "approved from Inbox", "hash"]);
  await decideProposal("s", A, "p1", true);
  assert.deepEqual(calls[1], ["proposal", "s", A, "p1", true]);
});
