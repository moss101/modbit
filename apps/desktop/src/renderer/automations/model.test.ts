import { test } from "node:test";
import assert from "node:assert/strict";
import type { AutoAttention, AutoRun, AutoView } from "../../shared/automation-types.ts";
import { approvalBlocked, attentionAction, durationWords, enableApprovalOf, nextDueOf, reasonWords, refusalOf, refusalSentence, relativeTo, runKinds, runStatusMeta, stateMeta, utcMinute, wouldHaveAskedOf, REASON_WORDS, STATE_META } from "./model.ts";

const H = "a".repeat(64);
const view = (over: Partial<AutoView> = {}): AutoView => ({
  automationId: "c".repeat(32), name: "nightly", description: "", currentVersion: 4, definitionHash: H, state: "NEEDS_APPROVAL", disabledReason: "", disabledDetail: "", paused: false, enabled: null,
  sourceKind: "local", sourcePath: "", workspaceRoot: "/r", principal: "creator", triggers: [{ id: "now", kind: "manual", summary: "manual / test run", nextDueMs: 0 }], effects: "reversible_write", capabilities: ["fs.write"], paths: ["reports/**"], hosts: [],
  consecutiveFailures: 0, lastRunMs: 0, lastRunStatus: "", definitionJson: "{}", needsListedApproval: true, contentChanged: false, queued: 0, active: 0, limitDeadlineMinutes: 30, limitMaxCostMinor: 0, limitApprovalWaitMinutes: 1440, ...over,
});
const run = (over: Partial<AutoRun> = {}): AutoRun => ({ dispatchKey: "k", automationId: "c".repeat(32), name: "n", version: 1, triggerId: "t", triggerKind: "schedule", eventId: "e", status: "succeeded", reason: "", detail: "", taskId: "", sessionId: "", principal: "creator", firedMs: 0, dispatchedMs: 0, finishedMs: 0, costMinor: 0, test: false, catchUp: false, missed: 0, findings: 0, outputsJson: "", acknowledged: false, slotMs: 0, ...over });

test("REQ-PX-086: the approval is exactly the version, hash and lists the Core reported for the version on screen", () => {
  const v = view();
  const a = enableApprovalOf(v);
  assert.deepEqual(a, { automationId: v.automationId, version: 4, definitionHash: H, effects: "reversible_write", capabilities: ["fs.write"], paths: ["reports/**"], hosts: [] });
  // The approval is a copy: editing it cannot alter what the view says.
  a.capabilities.push("net");
  assert.deepEqual(v.capabilities, ["fs.write"]);
});

test("REQ-PX-086: an already approved version or a changed repository file is not put up for approval", () => {
  assert.equal(approvalBlocked(view()), null);
  assert.match(approvalBlocked(view({ state: "ENABLED" })) ?? "", /already approved/);
  assert.match(approvalBlocked(view({ state: "PAUSED" })) ?? "", /already approved/);
  assert.match(approvalBlocked(view({ sourceKind: "repository", contentChanged: true })) ?? "", /Reload it from the repository/);
  assert.equal(approvalBlocked(view({ sourceKind: "repository", contentChanged: false })), null);
});

test("REQ-PX-086: every state, status and typed reason has words; an unknown one is shown as the Core sent it", () => {
  for (const s of ["ENABLED", "PAUSED", "DISABLED", "NEEDS_APPROVAL", "AUTO_DISABLED"]) assert.ok(STATE_META[s]?.words, s);
  assert.equal(stateMeta("SOMETHING_NEW").label, "SOMETHING_NEW");
  for (const r of ["DUPLICATE", "PAUSED", "BUDGET", "CONCURRENCY", "MISSED", "APPROVAL_EXPIRED", "KILLED", "REPLACED", "BUDGET_EXHAUSTED", "FILTER", "GATE", "POLICY"]) assert.ok(REASON_WORDS[r], r);
  assert.equal(reasonWords("WEIRD"), "WEIRD");
  assert.equal(reasonWords(""), "");
  assert.equal(runStatusMeta("skipped").label, "Skipped");
  assert.equal(runStatusMeta("never-heard").tone, "idle");
});

test("REQ-PX-086: a Core refusal is split into its code and message and named in words", () => {
  assert.deepEqual(refusalOf("Error invoking remote method 'automations:enable': Error: APPROVAL_MISMATCH: the approval names version 1"), { code: "APPROVAL_MISMATCH", message: "the approval names version 1" });
  assert.deepEqual(refusalOf("plain text"), { code: "", message: "plain text" });
  assert.match(refusalSentence("APPROVAL_LISTS_DIFFER: must list [..]"), /lists in the approval do not match/);
  assert.match(refusalSentence("REPOSITORY_UNTRUSTED: /r"), /not been trusted/);
  assert.equal(refusalSentence("ODD_CODE: detail"), "ODD_CODE: detail");
});

test("REQ-PX-086: time is shown in UTC and against the Core's own clock", () => {
  assert.equal(utcMinute(Date.UTC(2026, 9, 8, 3, 5)), "2026-10-08 03:05 UTC");
  assert.equal(utcMinute(0), "");
  const now = Date.UTC(2026, 9, 8, 3, 0);
  assert.equal(relativeTo(now + 4 * 60_000, now), "in 4 min");
  assert.equal(relativeTo(now - 2 * 3_600_000, now), "2 h ago");
  assert.equal(nextDueOf(view({ triggers: [{ id: "a", kind: "schedule", summary: "", nextDueMs: 900 }, { id: "b", kind: "schedule", summary: "", nextDueMs: 300 }, { id: "c", kind: "manual", summary: "", nextDueMs: 0 }] })), 300);
  assert.equal(nextDueOf(view()), 0);
  assert.equal(durationWords(run({ dispatchedMs: 1000, finishedMs: 66_000 })), "1 min 5 s");
  assert.equal(durationWords(run({ dispatchedMs: 1000, finishedMs: 0 })), "");
});

test("REQ-PX-086 AUT-E02: a dry run's outputs list what would have been asked; garbage lists nothing", () => {
  const w = wouldHaveAskedOf(JSON.stringify({ dry_run: true, task_state: "Completed", would_have_asked: [{ tool: "fs.write", outcome: "WOULD_HAVE_ASKED", decision: "Ask" }, { tool: "shell.exec", outcome: "DENIED", decision: { reason: "x" } }] }));
  assert.equal(w?.dryRun, true);
  assert.deepEqual(w?.entries.map((e) => [e.tool, e.outcome]), [["fs.write", "WOULD_HAVE_ASKED"], ["shell.exec", "DENIED"]]);
  assert.equal(wouldHaveAskedOf(""), null);
  assert.equal(wouldHaveAskedOf("{nope"), null);
  assert.deepEqual(wouldHaveAskedOf('{"dry_run":true}')?.entries, []);
  assert.equal(wouldHaveAskedOf("[1]")?.entries.length, 0);
});

test("REQ-PX-086: each attention item offers the action that clears it", () => {
  const a = (kind: string, action: string): AutoAttention => ({ kind, automationId: "x", name: "n", dispatchKey: "k", taskId: "t", reason: "", action, atMs: 0 });
  assert.equal(attentionAction(a("RUN_FAILED", "AckAutomationAttention")).kind, "acknowledge");
  assert.equal(attentionAction(a("SKIPPED_BUDGET", "AckAutomationAttention")).kind, "acknowledge");
  assert.equal(attentionAction(a("APPROVAL_EXPIRED", "AckAutomationAttention")).kind, "acknowledge");
  assert.equal(attentionAction(a("PARKED_APPROVAL", "ResolveApproval")).kind, "approval");
  assert.equal(attentionAction(a("AUTO_DISABLED", "EnableAutomation")).kind, "review");
  assert.match(attentionAction(a("SOURCE_CHANGED", "EnableAutomation")).label, /again/);
});

test("REQ-PX-086: which triggers a definition can be run by hand, tested with a payload, or only fire on their own", () => {
  const k = runKinds(view({ triggers: [{ id: "m", kind: "manual", summary: "", nextDueMs: 0 }, { id: "s", kind: "schedule", summary: "", nextDueMs: 0 }, { id: "p", kind: "event", summary: "", nextDueMs: 0 }, { id: "w", kind: "webhook", summary: "", nextDueMs: 0 }] }));
  assert.equal(k.manual?.id, "m");
  assert.deepEqual(k.payload.map((t) => t.id), ["p", "w"]);
  assert.deepEqual(k.schedule.map((t) => t.id), ["s"]);
  assert.equal(runKinds(view({ triggers: [] })).manual, null);
});
