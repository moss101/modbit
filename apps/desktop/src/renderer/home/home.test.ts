/**
 * REQ-PX-205: what home's waiting list may decide in place, the one decision path it shares with the task's card, and the
 * first-run steps' count. The retained mutation: a list that tried to allow an effect that cannot be undone is refused before
 * anything reaches the bridge.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { decideFromList, resolveShown } from "../approvals/decide.ts";
import { decidableFromList, wantsLine } from "../approvals/model.ts";
import { attentionWords, stepProgress } from "./model.ts";

const approval = (over: Partial<DockApprovalView> = {}): DockApprovalView => ({
  approvalId: "ap1",
  taskId: "t1",
  toolCallId: "c1",
  toolName: "shell.exec",
  effectClass: "ProtectedWrite",
  intentHash: "h".repeat(64),
  requestedAtMs: 1,
  expiresAtMs: 0,
  reason: { code: "NOT_IN_ALLOWLIST", askClasses: [], contained: false, runMode: "ALLOWLIST" } as unknown as DockApprovalView["reason"],
  intent: { argv: ["git", "remote", "add", "origin1", "https://example.invalid/one.git"], command: "", paths: [], hosts: [], escalation: "none", cwd: "", found: true, argsPreview: "{}", truncated: false } as unknown as DockApprovalView["intent"],
  computer: null,
  ...over,
});

/** A bridge that records what reached it. */
function fakeBridge(): { calls: unknown[][] } {
  const calls: unknown[][] = [];
  (globalThis as unknown as { window: unknown }).window = {
    modbit: {
      resolveApproval: async (...args: unknown[]) => {
        calls.push(args);
        return { approvalId: args[1], status: args[2] ? "APPROVED" : "DENIED", offset: "9" };
      },
    },
  };
  return { calls };
}

test("a protected write is decidable from a list; what cannot be undone, native control and a budget raise are not, each with its reason", () => {
  assert.deepEqual(decidableFromList(approval()), { ok: true });
  for (const effectClass of ["Destructive", "ExternalSideEffect"]) {
    const r = decidableFromList(approval({ effectClass }));
    assert.equal(r.ok, false, effectClass);
    assert.match((r as { why: string }).why, /cannot be undone.*asks twice/);
  }
  assert.equal(decidableFromList(approval({ toolName: "computer.click" })).ok, false);
  assert.equal(decidableFromList(approval({ computer: {} as DockApprovalView["computer"] })).ok, false);
  assert.equal(decidableFromList(approval({ toolName: "task.budget.raise" })).ok, false);
});

test("the list says what the effect wants: the exact command, else the action in words; never the tool's own name", () => {
  assert.equal(wantsLine(approval()), "Wants to run git remote add origin1 https://example.invalid/one.git");
  assert.equal(wantsLine(approval({ intent: { ...approval().intent, argv: ["git", "commit", "-m", "two words"] } })), 'Wants to run git commit -m "two words"');
  assert.equal(wantsLine(approval({ toolName: "git.push", intent: { ...approval().intent, argv: null as unknown as string[], command: "" } })), "Wants to: push commits");
});

test("a list decision goes through the card's path, bound to the intent hash the list showed", async () => {
  const bridge = fakeBridge();
  await decideFromList("s1", approval(), true);
  await decideFromList("s1", approval({ approvalId: "ap2" }), false);
  assert.deepEqual(
    bridge.calls.map((c) => [c[0], c[1], c[2], c[4]]),
    [
      ["s1", "ap1", true, "h".repeat(64)],
      ["s1", "ap2", false, "h".repeat(64)],
    ],
  );
  // The card's own path is the same call.
  await resolveShown("s1", approval({ approvalId: "ap3" }), true, "approved on the approval card");
  assert.deepEqual(bridge.calls[2]!.slice(0, 3), ["s1", "ap3", true]);
});

test("retained mutation: a list that tries to allow (or skip) an effect that cannot be undone is refused and nothing reaches the bridge", async () => {
  const bridge = fakeBridge();
  for (const approve of [true, false]) await assert.rejects(decideFromList("s1", approval({ effectClass: "Destructive" }), approve), /NOT_FROM_A_LIST: It cannot be undone/);
  await assert.rejects(decideFromList("s1", approval({ toolName: "computer.type" }), true), /NOT_FROM_A_LIST/);
  assert.equal(bridge.calls.length, 0);
});

test("the first-run steps count what is done and name the next one; done is a fact, never a click", () => {
  assert.deepEqual(stepProgress({ provider: false, repository: false, firstTask: false }), { done: 0, total: 3, next: "provider" });
  assert.deepEqual(stepProgress({ provider: true, repository: false, firstTask: false }), { done: 1, total: 3, next: "repository" });
  assert.deepEqual(stepProgress({ provider: true, repository: true, firstTask: false }), { done: 2, total: 3, next: "firstTask" });
  assert.deepEqual(stepProgress({ provider: true, repository: false, firstTask: true }), { done: 2, total: 3, next: "repository" });
  assert.deepEqual(stepProgress({ provider: true, repository: true, firstTask: true }), { done: 3, total: 3, next: "" });
});

test("what else waits on the person is said in plain words, never the Core's command names", () => {
  for (const kind of ["QUESTION", "CAPACITY", "STALL", "RESTART", "CONFLICT", "PROTECTED_EFFECT", "BLOCKER", "FAILURE", "SOMETHING_NEW"]) {
    const words = attentionWords({ kind, reason: "which port?" });
    assert.doesNotMatch(words, /StartTask|RespondToQuestion|CancelTask|QueueInput|MODBIT_|_/, kind);
    assert.ok(words.length > 0);
  }
  assert.equal(attentionWords({ kind: "QUESTION", reason: "which port?" }), "Asks you: which port?");
});
