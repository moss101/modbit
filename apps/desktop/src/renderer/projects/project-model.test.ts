import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import type { ProjectInfo, ProjectRollupInfo } from "../../shared/project-types.ts";
import { ciWords, colorVar, memberCiWords, newCommandId, parseRefusal, projectMenuFor, refusalSentence, rollupWords, workspaceChoices } from "./project-model.ts";

const roll = (over: Partial<ProjectRollupInfo> = {}): ProjectRollupInfo => ({ members: 0, byStatus: [], attentionTasks: 0, attentionItems: 0, pendingApprovals: 0, unread: 0, pullRequests: 0, ciPassing: 0, ciFailing: 0, ciPending: 0, ...over });
const project = (over: Partial<ProjectInfo> = {}): ProjectInfo => ({ projectId: "p1", name: "Alpha", color: "accent", icon: "folder", workspaceRoot: "/w/a", archived: false, createdAtMs: 1, updatedAtMs: 1, createdOffset: "1", lastOffset: "1", rollup: roll(), members: [], ...over });
const header = (over: Partial<AgentHeaderView> = {}): AgentHeaderView => ({ taskId: "t1", sessionId: "s", workspaceRoot: "/w/a", title: "A task", subtitle: "a", createdAtMs: 1, updatedAtMs: 1, statusClass: "RUNNING", statusLabel: "Running", unread: false, pendingApproval: false, pendingPlan: false, contextPercent: 0, filesChanged: 0, linesAdded: 0, linesRemoved: 0, lastCheckpointAtMs: 0, subagent: false, archived: false, executionLocation: "local", origin: "desktop", taskState: "Running", lastOffset: "1", readOffset: "0", attentionItems: 0, ...over });

test("a project colour is a palette role mapped to a design token variable; an unknown role is the accent, never a literal colour", () => {
  assert.equal(colorVar("warn"), "var(--mb-color-warn)");
  assert.equal(colorVar("modeAsk"), "var(--mb-color-mode-ask)");
  assert.equal(colorVar("#ff0000"), "var(--mb-color-accent)");
  assert.equal(colorVar("red; background:url(x)"), "var(--mb-color-accent)");
});

test("the rollup is said only from the counts the Core gave", () => {
  assert.equal(rollupWords(roll()), "0 tasks · none need attention".replace(" · none need attention", ""));
  assert.equal(rollupWords(roll({ members: 3 })), "3 tasks · none need attention");
  assert.equal(rollupWords(roll({ members: 1, attentionTasks: 1 })), "1 task · 1 needs attention");
  assert.equal(rollupWords(roll({ members: 5, attentionTasks: 2, pendingApprovals: 1, pullRequests: 2 })), "5 tasks · 2 need attention · 1 approval waiting · 2 pull requests");
});

test("CI is said only for pull requests the log holds, and a pull request without CI evidence says so", () => {
  assert.equal(ciWords(roll()), null);
  assert.equal(ciWords(roll({ pullRequests: 3, ciPassing: 1, ciFailing: 1 })), "1 passing, 1 failing, 1 with no CI result recorded");
  assert.equal(memberCiWords({ hasCi: false, checksPassed: 0, checksFailed: 0, checksPending: 0 }), "no CI result recorded");
  assert.equal(memberCiWords({ hasCi: true, checksPassed: 2, checksFailed: 1, checksPending: 0 }), "1 failed, 2 passed");
});

test("the Core's typed refusal is read out of the IPC error and led by Modbit's words", () => {
  const e = new Error("Error invoking remote method 'projects:addMember': Error: IN_ANOTHER_PROJECT: the task is in `Beta`; remove it from there first");
  assert.deepEqual(parseRefusal(e), { code: "IN_ANOTHER_PROJECT", detail: "the task is in `Beta`; remove it from there first" });
  const s = refusalSentence("Could not add “T” to “A”", e);
  assert.ok(s.startsWith("Could not add “T” to “A”: A task is in one project at a time."), s);
  assert.ok(s.endsWith("remove it from there first"), s);
  // No code: the text is shown as it is.
  assert.deepEqual(parseRefusal(new Error("socket closed")), { code: "", detail: "socket closed" });
  assert.equal(refusalSentence("x", new Error("boom")), "x: boom");
});

test("the project menu offers every live project (the Core refuses what is not allowed) and the way out of the current one", () => {
  const a = project({ projectId: "a", name: "Alpha" });
  const b = project({ projectId: "b", name: "Beta" });
  const old = project({ projectId: "c", name: "Old", archived: true });
  const free = projectMenuFor(header(), [a, b, old]);
  assert.deepEqual(free.map((m) => m.label), ["Add to Alpha", "Add to Beta"]);
  const member = projectMenuFor(header({ projectId: "a" }), [a, b, old]);
  assert.deepEqual(member.map((m) => [m.kind, m.label, m.checked]), [["add", "Add to Alpha", true], ["add", "Add to Beta", false], ["remove", "Remove from Alpha", false]]);
  // A member of an archived project can still be taken out of it.
  assert.equal(projectMenuFor(header({ projectId: "c" }), [a, old]).some((m) => m.kind === "remove"), true);
});

test("workspace choices: the task's own first, then the tasks' and projects' workspaces once each; a subagent's is not offered", () => {
  const rows = [header({ workspaceRoot: "/w/b" }), header({ workspaceRoot: "/w/a" }), header({ workspaceRoot: "/w/a" }), header({ workspaceRoot: "/w/sub", subagent: true })];
  assert.deepEqual(workspaceChoices(rows, [project({ workspaceRoot: "/w/c" })], "/w/a"), ["/w/a", "/w/b", "/w/c"]);
  assert.deepEqual(workspaceChoices([], []), []);
});

test("a command id is 32 hex characters and never repeats", () => {
  const ids = new Set(Array.from({ length: 50 }, newCommandId));
  assert.equal(ids.size, 50);
  for (const id of ids) assert.match(id, /^[0-9a-f]{32}$/);
});
