import { test } from "node:test";
import assert from "node:assert/strict";
import { create } from "@bufbuild/protobuf";
import { AgentStatusClass, ApplyConflictViewSchema, ProjectMemberViewSchema, ProjectPullRequestViewSchema, ProjectRollupSchema, ProjectStatusCountSchema, ProjectViewSchema, WorktreeApplyAckSchema, WorktreeViewSchema } from "@modbit/surface-protocol";
import { applyAckInfo, projectInfo, registerProjectHandlers, validApplyInput, validProjectInput, validProjectPatch, worktreeInfo } from "./project-ipc.ts";
import type { Registrar } from "./conversation-ipc.ts";

const SID = "0123456789abcdef0123456789abcdef";
const TID = "fedcba9876543210fedcba9876543210";
const PID = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/** A registrar whose Core client explodes when touched: a handler that reaches it has not finished validating. */
function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  let touched = 0;
  const r: Registrar = {
    handle: (channel, fn) => void handlers.set(channel, fn),
    client: () => {
      touched++;
      throw new Error("CLIENT_TOUCHED");
    },
    sessionId: (v) => {
      if (v !== SID) throw new Error("BAD_ARGUMENT: session id must be 32 hex chars");
      return SID;
    },
    taskId: (v) => {
      if (v !== TID) throw new Error("BAD_ARGUMENT: task id must be 32 hex chars");
      return TID;
    },
    lease: () => Promise.resolve(),
  };
  registerProjectHandlers(r);
  const call = async (channel: string, ...args: unknown[]): Promise<string> => {
    const fn = handlers.get(channel);
    assert.ok(fn, `${channel} is registered`);
    try {
      await fn(...args);
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  };
  return { call, touched: () => touched, channels: () => [...handlers.keys()] };
}

test("every project and worktree channel is registered, and none takes a raw path to act on or a command string", () => {
  const h = harness();
  assert.deepEqual(h.channels().sort(), [
    "projects:addMember", "projects:archive", "projects:create", "projects:get", "projects:list", "projects:removeMember", "projects:rename",
    "worktrees:apply", "worktrees:cleanup", "worktrees:discard", "worktrees:list", "worktrees:remove", "worktrees:undo",
  ]);
});

test("a malformed argument is refused in main before the Core client is touched (REQ-EV-0103)", async () => {
  const h = harness();
  const bad = async (channel: string, ...args: unknown[]) => assert.match(await h.call(channel, ...args), /BAD_ARGUMENT|CLIENT_TOUCHED/);
  // Session and task ids
  assert.match(await h.call("projects:create", "nope", { name: "A", workspaceRoot: "/w" }), /BAD_ARGUMENT: session id/);
  assert.match(await h.call("projects:addMember", SID, PID, "nope"), /BAD_ARGUMENT: task id/);
  // Project ids, names, colours, icons
  assert.match(await h.call("projects:get", "../etc"), /BAD_ARGUMENT: projectId/);
  assert.match(await h.call("projects:archive", SID, "short", true), /BAD_ARGUMENT: projectId/);
  assert.match(await h.call("projects:archive", SID, PID, "yes"), /BAD_ARGUMENT: archived/);
  assert.match(await h.call("projects:create", SID, { name: "  ", workspaceRoot: "/w" }), /BAD_ARGUMENT: name/);
  assert.match(await h.call("projects:create", SID, { name: "a\u0007b", workspaceRoot: "/w" }), /BAD_ARGUMENT: name/);
  assert.match(await h.call("projects:create", SID, { name: "A", color: "#ff0000", workspaceRoot: "/w" }), /BAD_ARGUMENT: color/);
  assert.match(await h.call("projects:create", SID, { name: "A", icon: "skull", workspaceRoot: "/w" }), /BAD_ARGUMENT: icon/);
  assert.match(await h.call("projects:create", SID, { name: "A", workspaceRoot: "" }), /BAD_ARGUMENT: workspaceRoot/);
  assert.match(await h.call("projects:create", SID, "not an object"), /BAD_ARGUMENT/);
  assert.match(await h.call("projects:create", SID, { name: "A", workspaceRoot: "/w" }, 12), /BAD_ARGUMENT: commandId/);
  assert.match(await h.call("projects:list", { includeArchived: "yes" }), /BAD_ARGUMENT: includeArchived/);
  // Worktrees
  assert.match(await h.call("worktrees:remove", SID, "", true), /BAD_ARGUMENT: worktreeId/);
  assert.match(await h.call("worktrees:remove", SID, "wt1", "yes"), /BAD_ARGUMENT: dryRun/);
  assert.match(await h.call("worktrees:apply", SID, TID, { option: "RM_RF" }), /BAD_ARGUMENT: option/);
  assert.match(await h.call("worktrees:apply", SID, TID, { option: "OVERWRITE", confirmPaths: "a.txt" }), /BAD_ARGUMENT: confirmPaths/);
  assert.match(await h.call("worktrees:apply", SID, TID, { expectedPlanDigest: "abc" }), /BAD_ARGUMENT: expectedPlanDigest/);
  assert.match(await h.call("worktrees:apply", SID, TID, { remember: "yes" }), /BAD_ARGUMENT: remember/);
  assert.match(await h.call("worktrees:discard", SID, TID, "why", ""), /BAD_ARGUMENT: confirm/);
  await bad("worktrees:cleanup", SID, { dryRun: "no" });
  // None of the refusals above reached the Core.
  assert.equal(h.touched(), 0);
  // A well-formed one does reach it (and here the stub client explodes).
  assert.match(await h.call("projects:create", SID, { name: "Alpha", color: "warn", icon: "rocket", workspaceRoot: "/w" }, "a".repeat(32)), /CLIENT_TOUCHED/);
});

test("argument narrowing keeps only the narrowest shape the Core takes", () => {
  assert.deepEqual(validProjectInput({ name: "Alpha", workspaceRoot: "/w", extra: "dropped" }), { name: "Alpha", color: "", icon: "", workspaceRoot: "/w" });
  assert.deepEqual(validProjectPatch({ icon: "leaf" }), { name: "", color: "", icon: "leaf" });
  assert.deepEqual(validApplyInput(undefined), { option: "", confirmPaths: [], remember: false, expectedCandidateRevision: 0n, expectedPlanDigest: "" });
  const ok = validApplyInput({ option: "STASH", confirmPaths: ["a", "b"], remember: true, expectedCandidateRevision: "7", expectedPlanDigest: "d".repeat(64) });
  assert.equal(ok.expectedCandidateRevision, 7n);
  assert.deepEqual(ok.confirmPaths, ["a", "b"]);
});

test("the Core's project view crosses as plain data: ids as hex, 64-bit counts as decimal strings, classes by name", () => {
  const v = create(ProjectViewSchema, {
    projectId: { value: new Uint8Array(16).fill(0xaa) },
    name: "Alpha",
    color: "warn",
    icon: "rocket",
    workspaceRoot: "/w",
    createdAtMs: 1_700_000_000_000n,
    lastOffset: 19n,
    rollup: create(ProjectRollupSchema, { members: 1, attentionTasks: 1, byStatus: [create(ProjectStatusCountSchema, { statusClass: AgentStatusClass.NEEDS_ATTENTION, label: "Needs attention", count: 1 })] }),
    members: [create(ProjectMemberViewSchema, { taskId: { value: new Uint8Array(16).fill(0xbb) }, title: "T", statusClass: AgentStatusClass.RUNNING, pullRequest: create(ProjectPullRequestViewSchema, { number: 42n, hasCi: true, checksFailed: 1 }) })],
  });
  const p = projectInfo(v);
  assert.equal(p.projectId, "aa".repeat(16));
  assert.equal(p.lastOffset, "19");
  assert.equal(p.createdAtMs, 1_700_000_000_000);
  assert.equal(p.rollup.byStatus[0]!.statusClass, "NEEDS_ATTENTION");
  assert.equal(p.members[0]!.taskId, "bb".repeat(16));
  assert.equal(p.members[0]!.statusClass, "RUNNING");
  assert.deepEqual(p.members[0]!.pullRequest, { number: "42", url: "", head: "", state: "", hasCi: true, checksPassed: 0, checksFailed: 1, checksPending: 0 });
  assert.doesNotThrow(() => structuredClone(p));
});

test("a worktree and an apply answer cross as plain data with the Core's flags untouched", () => {
  const w = worktreeInfo(create(WorktreeViewSchema, { worktreeId: "w1", taskId: { value: new Uint8Array(16).fill(1) }, bytes: 4096n, taskRunning: true, removable: false, removableReason: "its task has not ended" }));
  assert.deepEqual([w.worktreeId, w.bytes, w.taskRunning, w.removable, w.removableReason], ["w1", "4096", true, false, "its task has not ended"]);
  const a = applyAckInfo(
    create(WorktreeApplyAckSchema, {
      status: "CONFLICT",
      candidateRevision: 3n,
      conflict: create(ApplyConflictViewSchema, { planDigest: "d".repeat(64), candidateRevision: 3n, conflictingPaths: ["a.txt"], defaultOption: "CANCEL", options: [{ option: "CANCEL", available: true, destructive: false, needsConfirmation: false, confirmPaths: [], whyUnavailable: "" }] }),
    }),
  );
  assert.equal(a.status, "CONFLICT");
  assert.equal(a.candidateRevision, "3");
  assert.equal(a.conflict?.defaultOption, "CANCEL");
  assert.deepEqual(a.conflict?.conflictingPaths, ["a.txt"]);
  assert.doesNotThrow(() => structuredClone(a));
});
